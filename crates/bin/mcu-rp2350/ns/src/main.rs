//! RP2350 (Cortex-M33) NS-world MCU shipping binary skeleton (EPIC #350): Seplos/UART poll loop +
//! device-hosted MQTT broker (W5500) + DDS/RTPS discovery. Stage 4 demo scaffold (RP2350 A4),
//! signed boot + load-time integrity only — see `docs/decisions/0010-*.md`/`0011-*.md`.
//!
//! Features: `core1-parser` runs the parser on core 1; `veneers` adds the S/NS TrustZone-M crypto
//! leg; `profile` adds span-registry reporting; `mesh` adds the Meshtastic UART1 leg
//! (`boot_mesh.rs`); `power` adds the INA3221 rail-draw leg on its own I2C0 (GP4/GP5, diagnostics
//! `diag/rails`, `boot_i2c.rs`); `ui` adds the OLED FeatherWing on its own I2C1 (GP2/GP3,
//! `boot_ui.rs`); `tls` (ADR 0013) is a fail-closed transport-security stub (`boot_tls.rs`).
//! `mesh`+`ui` are mutually exclusive — both claim GP24/GP25 (see `compile_error!` below).
#![no_std]
#![no_main]

// `mesh` (Heltec UART1 TX/RX, boot_mesh.rs) and `ui` (buttons A/B, boot_ui.rs) both claim
// GP24/GP25 on the Feather RP2350's header — a genuine pin-budget collision, not an oversight, so
// they are mutually exclusive by design rather than reassigned. Pick one; if a future board needs
// both concurrently, the pin budget must be re-derived (a different header revision or a GPIO
// expander), not worked around here.
#[cfg(all(feature = "mesh", feature = "ui"))]
compile_error!(
    "features `mesh` and `ui` are mutually exclusive: both claim GP24/GP25 (Heltec UART1 TX/RX \
     under `mesh`, buttons A/B under `ui`). Enable only one."
);

#[cfg(feature = "ui")]
use core::sync::atomic::AtomicU8;
use core::sync::atomic::{AtomicU32, Ordering};

use embassy_executor::Spawner;
use embassy_net::tcp::TcpSocket;
use embassy_net::Stack;
use embassy_rp::bind_interrupts;
use embassy_rp::dma;
use embassy_rp::uart::BufferedInterruptHandler;
use embassy_rp::watchdog::Watchdog;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
// WP5 (delight-features): `UI_SNAPSHOT_WATCH`'s type.
#[cfg(feature = "ui")]
use embassy_sync::watch::Watch;
// `pub(crate)` so `core1.rs` (feature `core1-parser`) can name `RunConfig` for `spawn_parser_core1`'s
// signature without relying on the implicit "private-but-descendant-visible" default.
pub(crate) use impl_app::{run, RunConfig};
use lib_mapping::schema::Identity;
use lib_mapping::sink::MAX_PAYLOAD;
use lib_net::channel::MqttUnit;
// `Receiver` (trait) brings `.recv()` into scope for `seal_task`'s channel consume (feature
// `veneers` only). `broker_task` no longer names it directly: it hands its `rx` to the generic
// `async_broker_pump`, whose own `R: Receiver` bound resolves against lib-net's embassy-sync impl
// without needing the trait in THIS module's scope. Gated so the default build (no `seal_task`)
// doesn't warn on an unused import under `-D warnings`.
#[cfg(feature = "veneers")]
use lib_net::channel::Receiver;
use lib_net::embassy::pump::{async_broker_pump, BrokerEvent, LatestStore};
use lib_net::embassy::wait_for_config;
// Only rendered over RTT, so used solely by the defmt-gated IPv6 log in `broker_task`.
#[cfg(feature = "defmt")]
use lib_net::embassy::link_local_v6;
use lib_net::link::BrokerCore;
// WP5 (delight-features): named here (not just inferred) because `UI_SNAPSHOT_WATCH`'s `static`
// declaration below needs the type spelled out. `ns::ui` is the LIB target's module (see
// `lib.rs`'s doc comment) — this `use`, not `mod ui;`, is deliberately how this bin target reaches
// it, so the module exists exactly once across the lib/bin crate boundary.
#[cfg(feature = "ui")]
use ns::ui::UiSnapshot;
// Panic handler: silent halt in the default/ship build; but under `defmt` (the bench build) use
// panic-probe's defmt printer so a panic's message + `file:line` come out over RTT — essential on a
// console-less board (a bare `panic-halt` just freezes, indistinguishable from a hang).
#[cfg(not(feature = "defmt"))]
use panic_halt as _;
#[cfg(feature = "defmt")]
use panic_probe as _;
use static_cell::StaticCell;

// Under the `defmt` feature, link the RTT `#[global_logger]` so a debug/bench build actually has a
// defmt sink (dead-stripped without this `use`). UART0 carries the Seplos wire, not a console, so
// this is opt-in only — never the shipping default.
#[cfg(feature = "defmt")]
use defmt_rtt as _;

// Stage 4 (#355) additions — see the crate doc comment above for what each feature does.
// (`Receiver` is now imported unconditionally above — the always-compiled `broker_task` uses it.)
#[cfg(feature = "veneers")]
use lib_net::channel::{CryptoOp, CryptoStage, CryptoUnit, Stage};
#[cfg(feature = "veneers")]
use lib_security::KeyHandle;

mod boot_seplos;
// TMS device-identity + Required-STOR-state emitter (`DeviceInfo`, `Heartbeat`,
// `EnergyStartStopState`, `PowerPortState`, `DcLoadSharingState`, `ActiveMicrogridControllerState`)
// — the topics with no per-reading dependency, on their own cadence. Always compiled (mission TMS
// parity with `bin-rpi`, not a delight feature). See this module's own doc comment.
mod boot_state;
mod boot_w5500;
// WP3 (delight-features): Meshtastic mesh-radio leg (UART1 -> Heltec V4). Default OFF; see the
// crate doc comment above and this module's own doc comment for the feature's dependency chain.
#[cfg(feature = "mesh")]
mod boot_mesh;
// WP4 (delight-features): the INA3221 power-monitor's OWN directly-owned I2C0 bus + its bring-up/
// poll task. `ui` no longer needs this module — it owns a separate I2C1 bus of its own, built
// directly in `boot_ui.rs`. See this module's own doc comment for why the two legs no longer share
// a bus.
#[cfg(feature = "power")]
mod boot_i2c;
// WP5 (delight-features): the SSD1306 bring-up, render-on-change display task, and debounced
// button task. Default OFF; see this module's own doc comment.
#[cfg(feature = "ui")]
mod boot_ui;
// HIL debugging aid: onboard NeoPixel status LED bring-up + task (PIO0/DMA_CH2/GPIO21). Default
// OFF; see this module's own doc comment for the color/blink semantics.
#[cfg(feature = "neopixel")]
mod boot_neopixel;
// ADR 0013 (transport security): the TLS interposition point between the accepted `:1883`
// `TcpSocket` and the broker pump. Default OFF, and a fail-closed STUB — with the feature ON the
// broker refuses every connection rather than falling back to plaintext. See this module's own doc
// comment and `Cargo.toml`'s `tls` feature comment.
#[cfg(feature = "tls")]
mod boot_tls;
#[cfg(feature = "core1-parser")]
mod core1;
mod dds_pub;
mod impl_buffered_uart_adapter;
#[cfg(feature = "defmt")]
mod impl_defmt_log_bridge;
mod impl_embassy_clock;
mod impl_parser_buffer_pool;
mod impl_pool_reservation;
mod impl_pooled_socket;
#[cfg(feature = "profile")]
mod impl_profile_clock;
#[cfg(feature = "profile")]
mod impl_profile_sink;
// WP3 (delight-features): the mesh-radio leg's `impl_*.rs` behavior files — `Rp2350MeshDe`'s
// no-op `OutputPin`/`ErrorType` impls and `FromRadioParser`'s `new`/`feed`/`Default`. Gated on
// `mesh` exactly like the `types/` files they implement (`types/mod.rs`'s own
// `#[cfg(feature = "mesh")]` block); without these declarations the impl files sat on disk
// uncompiled and `FromRadioParser::new()` did not exist.
#[cfg(feature = "mesh")]
mod impl_rp2350_mesh_de;
#[cfg(feature = "mesh")]
mod impl_rp2350_mesh_parser;
mod impl_socket_buffer_pool;
// ADR 0013: `TlsRefusal::reason()`. Gated on `tls` exactly like the `types/` file it implements.
#[cfg(feature = "tls")]
mod impl_tls_refusal;
#[cfg(feature = "veneers")]
mod impl_veneer_session_cipher;
mod mpu;
mod prelude;
mod spdp;
mod types;

#[cfg(feature = "veneers")]
use types::VeneerSessionCipher;
use types::{ParserBufferPool, PooledSocket, SocketBufferPool};
// Incidental fix (unrelated to WP4/WP5): `profiler_task` (feature `profile`, below) names
// `EmbassyClock` bare with no import anywhere in this file — a pre-existing unresolved-path bug,
// presumably never caught because no recent `cargo build --features profile` ran against this
// excluded workspace (the kellnr-mirror blocker predates this wave). Found incidentally while
// reviewing this file; fixed since it is a one-line import and this file is this wave's to edit.
#[cfg(feature = "profile")]
use types::EmbassyClock;
// Concrete monomorphizations of the generic seam types — embassy tasks cannot be generic, so the
// spawned task functions need one concrete instantiation per platform (this crate's only consumer
// of `bin_mcu_platform::McuPlatform`/`ChannelPublisher`). Each alias lives in its own file under
// `types/` (see that module); re-exported here at the crate root (`pub(crate)`) so both the bare
// `Rp2350*` names used in this file AND the `crate::Rp2350*` paths used in `boot_seplos.rs` /
// `boot_w5500.rs` / `core1.rs` resolve unchanged.
pub(crate) use types::{
    Rp2350De, Rp2350NetRunner, Rp2350Platform, Rp2350Receiver, Rp2350Sender, Rp2350SpiDevice,
    Rp2350Uart, Rp2350W5500Parts, Rp2350WiznetRunner,
};
// WP3 (delight-features): `Rp2350MeshSender` is named explicitly (not just inferred) in `run_task`
// below, mirroring `Rp2350Sender`/`Rp2350Receiver`'s own explicit-type convention; the other two
// are re-exported at the crate root for the SAME reason the non-mesh `Rp2350*` aliases above are —
// `boot_mesh.rs` spells them `crate::Rp2350MeshReceiver`/`crate::Rp2350MeshTransport`.
#[cfg(feature = "mesh")]
pub(crate) use types::{Rp2350MeshReceiver, Rp2350MeshSender, Rp2350MeshTransport};

// `seal_task` (below) relies on `CryptoUnit::data` and `MqttUnit::payload` being the exact same
// monomorphized `heapless::Vec<u8, 8192>` type so a sealed/opened `CryptoUnit` can move straight
// into/out of an `MqttUnit` with no copy — this const-asserts that dependency at compile time
// rather than leaving it an implicit coincidence: if `lib-net` ever lets these two consts diverge,
// this line (not a silent truncation at runtime) is what breaks.
#[cfg(feature = "veneers")]
const _: () = assert!(
    lib_net::channel::message::CRYPTO_MAX == lib_net::channel::message::MQTT_PAYLOAD,
    "seal_task assumes CryptoUnit::data and MqttUnit::payload share one capacity"
);

bind_interrupts!(struct Irqs {
    UART0_IRQ => BufferedInterruptHandler<embassy_rp::peripherals::UART0>;
    // WP3 (delight-features): the Heltec mesh-radio UART leg (`boot_mesh.rs`). Default OFF.
    #[cfg(feature = "mesh")]
    UART1_IRQ => BufferedInterruptHandler<embassy_rp::peripherals::UART1>;
    // DMA_CH2's `ChannelInstance::Interrupt` is hardwired to DMA_IRQ_0 (verified: embassy-rp's own
    // trait bound rejects binding it to any other IRQ name) — so the `neopixel` feature's DMA_CH2
    // handler must join THIS line, not a separate one. `bind_interrupts!` supports a per-handler
    // `#[cfg(...)]` inside one IRQ's comma list (see embassy-rp's own macro definition), so this
    // stays additive: CH0/CH1's always-compiled bindings are untouched, CH2's is compiled in only
    // under `neopixel`.
    DMA_IRQ_0 => dma::InterruptHandler<embassy_rp::peripherals::DMA_CH0>, dma::InterruptHandler<embassy_rp::peripherals::DMA_CH1>, #[cfg(feature = "neopixel")] dma::InterruptHandler<embassy_rp::peripherals::DMA_CH2>;
    // WP4 (delight-features): the INA3221's OWN directly-owned I2C0 bus (`boot_i2c.rs`, GP4/GP5).
    // Default OFF.
    #[cfg(feature = "power")]
    I2C0_IRQ => embassy_rp::i2c::InterruptHandler<embassy_rp::peripherals::I2C0>;
    // WP5 (delight-features): the OLED's OWN directly-owned I2C1 bus (`boot_ui.rs`, GP2/GP3) — a
    // DIFFERENT peripheral than `power`'s I2C0 above, so this is its own independent line rather
    // than joining it (contrast DMA_IRQ_0 above, where CH0/CH1/CH2 genuinely share one IRQ vector).
    // Default OFF.
    #[cfg(feature = "ui")]
    I2C1_IRQ => embassy_rp::i2c::InterruptHandler<embassy_rp::peripherals::I2C1>;
    // HIL debugging aid (`boot_neopixel.rs`): PIO0 drives the onboard NeoPixel's WS2812 timing.
    // Default OFF.
    #[cfg(feature = "neopixel")]
    PIO0_IRQ_0 => embassy_rp::pio::InterruptHandler<embassy_rp::peripherals::PIO0>;
});

/// Poll-loop liveness counter, bumped by `EmbassyClock::sleep_ms` each cycle. Gated to watchdog
/// to catch both wedged loops and executor starvation — see `watchdog_task` doc.
static POLL_HEARTBEAT: AtomicU32 = AtomicU32::new(0);

/// Device identity (fixed here until a config source lands).
///
/// TODO(ADR 0013): per-unit identity — hardcoded, will collide across units. This is the TMS IDL
/// `@key tms::Identity deviceId` on every published topic (see `docs/compliance/tms/`); two
/// devices shipped with this same value alias their telemetry onto ONE DDS instance (a subscriber
/// cannot tell them apart — later samples from unit B overwrite/interleave with unit A's under the
/// same key). Redesign in flight: derive per-unit identity from the device certificate via the
/// `TrustStore` seam (`docs/decisions/0013-transport-security-trust-store.md`) rather than a
/// baked-in literal. Do not fix here — a wrong fix now would conflict with that work item.
const DEVICE_ID: &str = "METEOR-002";

/// MQTT channel depth. Must absorb the largest **un-yielded** publish burst any producer emits
/// between suspension points, because [`bin_mcu_platform::ChannelPublisher::publish`] is `try_send`
/// (DROPS on a full channel) and the broker pump ([`lib_net::embassy::pump::async_broker_pump`])
/// only drains while a producer is suspended.
///
/// The binding burst is the SYNC `Platform::emit_reading` (`bin_mcu_platform::McuPlatform`), which
/// fires **3** publishes back-to-back with no `await` between them (`DcMeasurementUpdate` +
/// `PowerHardwareUpdate` + `StorageUpdate`) — a sync trait method can't yield, so depth is the only
/// lever. (`emit_fault`'s `ActiveDiagnosticState` is separated from that burst by a `read_fault`
/// UART `await`; `boot_state::state_task` and `boot_i2c::rail_task` both `yield_now().await` after
/// each publish, so each leaves at most one unit in flight.) Depth **6** = the 3-unit burst + margin
/// for a residual unit or two from the yielding producers. The original depth 2 silently dropped the
/// 3rd unit of every `emit_reading` burst (`StorageUpdate`) on hardware — this is that fix.
///
/// **SRAM.** Each `MqttUnit` is ~8.3 KiB (`MQTT_PAYLOAD` 8192 + `MQTT_TOPIC` 128 + tag), so this
/// channel is depth × ~8.3 KiB ≈ **50 KiB** at depth 6 (was ~16.6 KiB at depth 2; +33 KiB). Well
/// within the ~400 KiB free of 512 KiB. NOTE the per-unit size is ~8.3 KiB, not the ~2 KiB an
/// earlier estimate assumed — grow this constant deliberately. Under feature `veneers` the same
/// constant also sizes `SEALED_CHANNEL` (a second ~50 KiB); acceptable, and veneers is off in the
/// default/sign-off build. See `docs/decisions/` for backpressure tuning.
const MQTT_CHANNEL_DEPTH: usize = 6;

/// Poll-to-broker channel: `run_task` sender, `drain_task` receiver. `'static` storage.
static MQTT_CHANNEL: Channel<CriticalSectionRawMutex, MqttUnit, MQTT_CHANNEL_DEPTH> =
    Channel::new();

/// Sealed-output channel (feature `veneers` only): `seal_task` -> `drain_task`.
#[cfg(feature = "veneers")]
static SEALED_CHANNEL: Channel<CriticalSectionRawMutex, MqttUnit, MQTT_CHANNEL_DEPTH> =
    Channel::new();

/// Mesh-radio channel depth (feature `mesh` only, WP3 delight-features): `run_task`'s
/// `bin_mcu_platform::MeshPlatform` sender -> `boot_mesh::mesh_radio_task` receiver. Mirrors
/// `MQTT_CHANNEL_DEPTH`'s reasoning — depth 2 is headroom for one in-flight radio write + one
/// freshly-condensed frame, sufficient for a delight-feature secondary path with no backpressure
/// contract of its own (a full channel just drops the older frame — see `MeshPlatform::forward_to_mesh`).
#[cfg(feature = "mesh")]
const MESH_CHANNEL_DEPTH: usize = 2;

/// Poll-to-radio channel (feature `mesh` only): `run_task`'s `MeshPlatform` sender,
/// `boot_mesh::mesh_radio_task`'s receiver. `'static` storage, same convention as `MQTT_CHANNEL`.
#[cfg(feature = "mesh")]
static MESH_CHANNEL: Channel<
    CriticalSectionRawMutex,
    bin_mcu_platform::MeshUnit,
    MESH_CHANNEL_DEPTH,
> = Channel::new();

/// UART1 TX/RX buffers for the Heltec mesh-radio leg (feature `mesh` only) — same convention as
/// `UART_TX_BUF`/`UART_RX_BUF` (the Seplos UART0 leg), sized via `boot_mesh::MESH_UART_BUF_LEN`.
#[cfg(feature = "mesh")]
static MESH_UART_TX_BUF: StaticCell<[u8; boot_mesh::MESH_UART_BUF_LEN]> = StaticCell::new();
#[cfg(feature = "mesh")]
static MESH_UART_RX_BUF: StaticCell<[u8; boot_mesh::MESH_UART_BUF_LEN]> = StaticCell::new();

/// Reserved render scratch for future MQTT-render step. `static`, never stack-local per repo
/// zero-copy rule. Unused in Stage 1 (drain only counts/drops); reserved for TODO(#352).
static PAYLOAD_SCRATCH: StaticCell<[u8; MAX_PAYLOAD]> = StaticCell::new();

/// `boot_state::state_task`'s own reused render scratch — separate from `PAYLOAD_SCRATCH`
/// (`broker_task`'s) and `RAIL_PAYLOAD_SCRATCH` (`rail_task`'s): three independent tasks never share
/// one buffer. Same "static, never a per-call stack local" convention; sized `MAX_PAYLOAD` so the
/// ~1300 B `DeviceInfo` payload fits with headroom.
static STATE_PAYLOAD_SCRATCH: StaticCell<[u8; MAX_PAYLOAD]> = StaticCell::new();

/// `boot_i2c::rail_task`'s own reused diagnostic-serialize scratch (feature `power` only) —
/// separate from `PAYLOAD_SCRATCH` above (that one is `broker_task`'s; two independent tasks never
/// share one buffer) but the SAME "static, never a per-call stack local" convention. Reused for the
/// small `diag/rails` JSON payload, NOT a TMS payload (this leg never touches `lib_mapping`).
#[cfg(feature = "power")]
static RAIL_PAYLOAD_SCRATCH: StaticCell<[u8; MAX_PAYLOAD]> = StaticCell::new();

/// The OLED FeatherWing's latest [`UiSnapshot`] (WP5, feature `ui` only) — an
/// `embassy_sync::watch::Watch`, NOT a `Channel` (WP5 task note): always-latest, non-blocking
/// `send`/`send_modify`, so a producer task (`boot_i2c::rail_task`) never stalls on the UI. `N = 1`:
/// only `boot_ui::display_task` would ever call `.receiver()` — today it polls via `.try_get()`
/// directly on the `Watch` instead, so this capacity is headroom, not a live requirement.
///
/// **`CriticalSectionRawMutex` is mandatory, not a choice.** A `static`'s type must be `Sync`, and
/// `NoopRawMutex` is deliberately NOT (`PhantomData<*mut ()>`) — no "single executor, so no
/// contention" argument can make it one. Same raw mutex as `MQTT_CHANNEL`.
#[cfg(feature = "ui")]
static UI_SNAPSHOT_WATCH: Watch<CriticalSectionRawMutex, UiSnapshot, 1> = Watch::new();

/// The currently-displayed [`ns::ui::Screen`], encoded as a `u8` (WP5, feature `ui` only) —
/// `boot_ui::screen_to_u8`/`screen_from_u8` convert. A plain `AtomicU8` rather than a second `Watch`:
/// `boot_ui::button_task` is the ONLY writer (so a load-then-store is race-free with no CAS loop
/// needed) and `boot_ui::display_task` just polls the latest value each render tick, same as it
/// already does for `UI_SNAPSHOT_WATCH`.
#[cfg(feature = "ui")]
static UI_CURRENT_SCREEN: AtomicU8 = AtomicU8::new(0);

/// UART0 TX/RX buffers (256 bytes each, matches Seplos frame size). `static` storage.
const UART_BUF_LEN: usize = 256;
static UART_TX_BUF: StaticCell<[u8; UART_BUF_LEN]> = StaticCell::new();
static UART_RX_BUF: StaticCell<[u8; UART_BUF_LEN]> = StaticCell::new();

/// Static parser-buffer pool scoped by `mpu.rs`'s `NS_PARSER` region. Always compiled (Stage-4
/// scaffold, not opt-in).
static PARSER_BUFFER_POOL: ParserBufferPool = ParserBufferPool::new();

/// Profile span registry (feature `profile` only). Fixed capacity 64 spans.
#[cfg(feature = "profile")]
pub(crate) static PROFILE_REGISTRY: lib_profile::SpanRegistry<64> =
    lib_profile::SpanRegistry::new();

/// Poll cadence (milliseconds) `run_config()` bakes into `RunConfig::interval_ms`. `DcMeasurementUpdate`
/// is spec QoS Profile Medium / Data Trigger "Periodic 1 s"
/// (`docs/compliance/tms/mil-std-3071_data_model.idl:358`, `docs/compliance/README.md:68/572`
/// §4.9.10.4) and — per `run_config()`'s own doc comment below — is NOT rate-gated on this path, so
/// this constant directly sets the real wire publish rate. Hoisted out of the `RunConfig { .. }`
/// literal (rather than left as a bare `1000`) purely so the compile-time assertion immediately
/// below can bind a name to it.
const POLL_INTERVAL_MS: u32 = 1000;

const _: () = assert!(
    POLL_INTERVAL_MS == 1000,
    "MIL-STD-3071 requires 1 Hz DcMeasurementUpdate publication (QoS Profile Medium / Periodic 1 s, \
     docs/compliance/tms/mil-std-3071_data_model.idl:358) — do not change POLL_INTERVAL_MS away from 1000"
);

/// Concurrent MQTT client slots. Peak TCP sockets = `MAX_CLIENTS` serving + 1 accepting.
const MAX_CLIENTS: usize = 3;

/// W5500 queue sizes.
const W5500_RX_QUEUE_SIZE: usize = 4;
const W5500_TX_QUEUE_SIZE: usize = 4;

/// smoltcp socket table capacity (app TCP sockets + DHCP + DNS infra). See `boot_w5500.rs`'s module
/// doc for the accounting this must hold: `MAX_CLIENTS` serving + 1 accepting + 1 DHCP (always
/// allocated under `Config::dhcpv4`) + 1 DNS (embassy-net may allocate one internally) is the
/// documented MINIMUM (`MAX_CLIENTS + 3`); this const adds 1 spare on top of that floor. The prior
/// shipped bug sized this `MAX_CLIENTS + 1` (no DHCP/DNS headroom at all) and silently capped live
/// clients at 2 — see `boot_w5500.rs` for the full incident writeup. The assertion below enforces
/// the floor at compile time so that bug class cannot silently recur.
const STACK_SOCKETS: usize = MAX_CLIENTS + 4; // = 7: 3 serving + 1 accepting + 1 DHCP + 1 DNS + 1 spare

const _: () = assert!(
    STACK_SOCKETS > MAX_CLIENTS + 2,
    "STACK_SOCKETS must leave room for 1 accepting + 1 DHCP + 1 DNS socket beyond MAX_CLIENTS \
     serving sockets (MAX_CLIENTS + 3 minimum) — undersizing this silently caps live clients \
     below MAX_CLIENTS (see boot_w5500.rs's STACK_SOCKETS-sizing-bug writeup)"
);

/// W5500 driver state (queued packets). `'static` via StaticCell.
static W5500_STATE: StaticCell<
    embassy_net_wiznet::State<W5500_RX_QUEUE_SIZE, W5500_TX_QUEUE_SIZE>,
> = StaticCell::new();

/// Embassy-net stack socket table. Sized to `STACK_SOCKETS` (not just `MAX_CLIENTS`).
static W5500_STACK_RESOURCES: StaticCell<embassy_net::StackResources<STACK_SOCKETS>> =
    StaticCell::new();

/// Device-hosted MQTT broker core. Holds one slot per `MAX_CLIENTS` (not sized to infra sockets).
static BROKER_CORE: StaticCell<BrokerCore<MAX_CLIENTS>> = StaticCell::new();

/// Latest-value store topic capacity. Sized to hold every DISTINCT topic this device publishes
/// without FIFO-evicting one (the store evicts the oldest entry when full — see
/// `lib_net::embassy::pump::LatestStore::upsert`). The full TMS parity set is 10 topics
/// (`DcMeasurementUpdate`, `PowerHardwareUpdate`, `StorageUpdate`, `ActiveDiagnosticState`,
/// `DeviceInfo`, `Heartbeat`, `EnergyStartStopState`, `PowerPortState`, `DcLoadSharingState`,
/// `ActiveMicrogridControllerState`) plus `diag/rails` (feature `power`) = 11; `12` leaves one
/// spare. Grown from the original `8` when the device-identity/state emitter (`boot_state.rs`) and
/// `StorageUpdate` landed — at `8` the four newest topics would have churned the store.
pub(crate) const STORE_TOPICS: usize = 12;

/// Latest-value store (topic → payload). `STORE_TOPICS` topics × `MAX_PAYLOAD`-byte payloads
/// (12 × 2048 = ~24 KiB of 512 KiB SRAM, vs. default 32×8192 = 260 KiB). Payload capacity is
/// `MAX_PAYLOAD` (not the former 1024) so ANY payload the render scratch can hold also fits the
/// store: `DeviceInfo` alone renders to ~1300 B (measured), which the old 1024 cap silently
/// dropped. Sized for SRAM budget.
static LATEST_STORE: StaticCell<LatestStore<STORE_TOPICS, MAX_PAYLOAD>> = StaticCell::new();

/// Socket RX scratch buffer (512 bytes, typical embedded broker frame size).
static W5500_RX_BUF: StaticCell<[u8; 512]> = StaticCell::new();

/// TcpSocket rx/tx buffer pool (`MAX_CLIENTS + 1` pairs). Reclaimed on socket drop. See
/// `types/socket_buffer_pool.rs` for reclaim + cancellation-safety.
static SOCKET_POOL: SocketBufferPool = SocketBufferPool::new();

pub(crate) fn run_config() -> RunConfig {
    let mut device_id = Identity::new();
    let _ = device_id.push_str(DEVICE_ID);
    let mut prefix = heapless::String::new();
    let _ = prefix.push_str("tms");
    let mut dip_addresses = heapless::Vec::new();
    let _ = dip_addresses.push(0u8);
    RunConfig {
        device_id,
        prefix,
        dip_addresses,
        // `DcMeasurementUpdate` is spec Medium / Periodic 1 s (MIL-STD-3071 §4.9.10.4) and is NOT
        // rate-gated on this path either — `bin_mcu_platform::McuPlatform::emit_reading` renders +
        // publishes it every poll cycle with no cadence limiter (unlike the Pi's
        // `bin_rpi_platform::EmitCadence`, which only gates the two 10 s topics). This
        // `interval_ms` therefore directly sets the real DcMeasurementUpdate publish rate — same
        // defect class as the Pi's shipped configs (see `crates/bin/rpi/src/config`); keep at
        // `POLL_INTERVAL_MS` (const-asserted == 1000 above) to meet the 1 Hz requirement. (The
        // separate `dds_pub_task` SEDP/DDS demo publishes a synthetic reading on its own
        // `dds_pub::PUBLISH_INTERVAL` — same 1 Hz requirement, independently const-asserted there —
        // unrelated to this field.)
        interval_ms: POLL_INTERVAL_MS,
        fault_every: 10,
        max_backoff_ms: 2_000, // Tight for MCU demo; Seplos UART link faults intermittently.
    }
}

/// Boot sequence. Call helpers in order: MPU -> Seplos -> parser task -> watchdog -> W5500 ->
/// broker/DDS tasks. See boot_seplos.rs and boot_w5500.rs for step details.
#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());

    // Wire log -> defmt/RTT before any task emits (run_task's diagnostics are silent otherwise).
    #[cfg(feature = "defmt")]
    {
        log::set_logger(&impl_defmt_log_bridge::LOGGER).ok();
        log::set_max_level(log::LevelFilter::Info);
        defmt::info!("ns: main entry");
    }

    // HIL debugging aid (default OFF): bring up the onboard NeoPixel status LED as early as
    // possible so it lights (dim white, "booting") for the whole MPU/Seplos/W5500 bring-up window
    // below. `neopixel` is kept alive as a plain local across this fn's `.await` points and handed
    // to `neopixel_task` once a `Stack` exists (see that task's own doc comment for why the
    // link/DHCP-aware colors live there instead of here).
    #[cfg(feature = "neopixel")]
    let neopixel = boot_neopixel::build_neopixel(p.PIO0, p.DMA_CH2, p.PIN_21).await;

    boot_seplos::init_mpu();

    // Seplos/UART: build transport, platform, spawn parser (core 0 or core1-parser).
    let transport = boot_seplos::build_seplos_transport(p.UART0, p.PIN_0, p.PIN_1, p.PIN_29);
    let (platform, rx) = boot_seplos::build_platform(transport);
    let drain_rx = boot_seplos::resolve_drain_rx(spawner, rx);
    #[cfg(not(feature = "core1-parser"))]
    boot_seplos::spawn_parser(spawner, platform);
    #[cfg(feature = "core1-parser")]
    boot_seplos::spawn_parser(p.CORE1, platform);

    // Meshtastic mesh-radio leg (WP3, delight-features): UART1 -> Heltec V4 Serial Module. Fail
    // open by construction — a chip that's absent/unresponsive is never probed at boot (unlike
    // W5500 below), it just never drains `MESH_CHANNEL`, and `MeshPlatform::forward_to_mesh`
    // (inside `run_task`) already treats a full/closed channel as a silent drop.
    #[cfg(feature = "mesh")]
    {
        let mesh_transport = boot_mesh::build_mesh_transport(p.UART1, p.PIN_24, p.PIN_25);
        spawner.spawn(
            boot_mesh::mesh_radio_task(mesh_transport, MESH_CHANNEL.receiver())
                .expect("failed to create mesh_radio_task token (pool exhausted)"),
        );
    }

    // INA3221 multi-rail rail-draw leg (WP4, delight-features): I2C0, SDA=GP4/SCL=GP5 — the
    // bench's actual INA3221 wiring, directly owned by this leg alone (no shared bus; see
    // `boot_i2c.rs`'s module doc for why). Reads the W5500/Heltec rail currents every 2s on its OWN
    // `Ticker` — deliberately NOT gated on `POLL_HEARTBEAT` (a stalled I2C read on this secondary
    // leg must never be able to reset the board via the watchdog). Adapter self-telemetry only —
    // published to the non-TMS `diag/rails` topic, never mixed into the Seplos-sourced TMS mission
    // telemetry. `tx` is a SECOND sender handle onto `MQTT_CHANNEL` (a general, not
    // single-producer, channel — see `boot_i2c::rail_task`'s own doc comment). Fail-open by
    // construction: building the bus never probes it (unlike W5500 below) — an absent/unresponsive
    // chip just never responds, handled entirely by `rail_task`'s own fail-open posture.
    #[cfg(feature = "power")]
    {
        let i2c0 = boot_i2c::build_i2c0(p.I2C0, p.PIN_4, p.PIN_5);
        let monitor = boot_i2c::build_rail_monitor(i2c0);
        let tx: Rp2350Sender = MQTT_CHANNEL.sender();
        let payload_scratch: &'static mut [u8; MAX_PAYLOAD] =
            RAIL_PAYLOAD_SCRATCH.init([0u8; MAX_PAYLOAD]);
        spawner.spawn(
            boot_i2c::rail_task(monitor, tx, run_config(), payload_scratch)
                .expect("failed to create rail_task token (pool exhausted)"),
        );
    }

    // OLED FeatherWing UI leg (WP5, delight-features): I2C1, SDA=GP2/SCL=GP3 — directly owned by
    // this leg alone (a DIFFERENT peripheral than the power leg's I2C0 above, so there is no bus to
    // share) + three debounced buttons (A=GP9/B=GP6/C=GP5 — B is the only one with an onboard
    // pull-up; `Pull::Up` on all three in `boot_ui::button_task` is what makes A/C fire at all).
    // Fail-open: `display_task` itself probes `.init()` and exits quietly if the chip never
    // responds.
    #[cfg(feature = "ui")]
    {
        let display = boot_ui::build_display(p.I2C1, p.PIN_2, p.PIN_3, p.PIN_6);
        spawner.spawn(
            boot_ui::display_task(display)
                .expect("failed to create display_task token (pool exhausted)"),
        );
        // `not(feature = "mesh")` on top of the block's own `feature = "ui"`: the `mesh`+`ui`
        // combination is already rejected above by the `compile_error!`, but without this the
        // borrow checker would still try to type-check THIS statement fighting `boot_mesh` above
        // over the same `p.PIN_24`/`p.PIN_25` (E0382), burying the useful compile_error under a
        // move-semantics error. Excluding it here keeps that combination's build failing with
        // ONLY the compile_error!'s message.
        #[cfg(not(feature = "mesh"))]
        spawner.spawn(
            boot_ui::button_task(p.PIN_24, p.PIN_25, p.PIN_27)
                .expect("failed to create button_task token (pool exhausted)"),
        );
    }

    // Watchdog spawned AFTER parser (needs POLL_HEARTBEAT producer to exist).
    spawner.spawn(
        watchdog_task(Watchdog::new(p.WATCHDOG))
            .expect("failed to create watchdog_task token (pool exhausted)"),
    );

    // TMS device-identity + Required-STOR-state emitter (always compiled — mission topic-parity with
    // `bin-rpi`, not a delight feature). A SECOND sender handle onto `MQTT_CHANNEL` (like
    // `rail_task`'s) drives `DeviceInfo` + `Heartbeat` + the four static state topics on their own
    // cadence, independent of the Seplos poll cycle. Fail-open: early publishes onto a not-yet-
    // drained channel just drop (broker isn't up until DHCP); the task's 1 s heartbeat + 60 s
    // DeviceInfo/state refresh re-assert every topic once the broker's `LatestStore` is live.
    {
        let state_tx: Rp2350Sender = MQTT_CHANNEL.sender();
        let state_scratch: &'static mut [u8; MAX_PAYLOAD] =
            STATE_PAYLOAD_SCRATCH.init([0u8; MAX_PAYLOAD]);
        spawner.spawn(
            boot_state::state_task(state_tx, run_config(), state_scratch)
                .expect("failed to create state_task token (pool exhausted)"),
        );
    }

    #[cfg(feature = "defmt")]
    defmt::info!("ns: booted, bringing up W5500 (SPI0)…");

    // W5500 bring-up (SPI0). Fail open if chip absent/mis-wired (broker is secondary, not primary mission).
    let w5500_parts = match boot_w5500::build_w5500(
        p.SPI0, p.PIN_22, p.PIN_23, p.PIN_20, p.DMA_CH0, p.DMA_CH1, p.PIN_10, p.PIN_11, p.PIN_26,
    )
    .await
    {
        Ok(parts) => parts,
        Err(_e) => {
            #[cfg(feature = "defmt")]
            defmt::error!("W5500 init failed");
            return;
        }
    };
    // Copy stack handle before spawn_net_and_broker moves w5500_parts (DDS needs it too).
    let stack = w5500_parts.stack;
    boot_w5500::spawn_net_and_broker(spawner, w5500_parts, drain_rx);

    // HIL debugging aid (default OFF): hand the NeoPixel driver + `Stack` to its status task now
    // that a `Stack` exists — see `boot_neopixel.rs`'s module doc for the link/DHCP/telemetry color
    // semantics.
    #[cfg(feature = "neopixel")]
    spawner.spawn(
        boot_neopixel::neopixel_task(neopixel, stack)
            .expect("failed to create neopixel_task token (pool exhausted)"),
    );

    // SPDP discovery (EPIC #328, step 1): ff02::1234 participant announcement over UDP/IPv6.
    spawner.spawn(
        spdp::spdp_task(stack, boot_w5500::W5500_MAC)
            .expect("failed to create spdp_task token (pool exhausted)"),
    );

    // DDS user-data (EPIC #328, step 2): publish TMS DcMeasurementUpdate samples over UDP/IPv6.
    spawner.spawn(
        dds_pub::dds_pub_task(stack, boot_w5500::W5500_MAC)
            .expect("failed to create dds_pub_task token (pool exhausted)"),
    );

    // Profiler reporting (feature profile only): periodic span report over RTT.
    #[cfg(feature = "profile")]
    spawner.spawn(profiler_task().expect("failed to create profiler_task token (pool exhausted)"));

    // Idle forever (all work runs in spawned tasks).
    loop {
        embassy_time::Timer::after(embassy_time::Duration::from_secs(1)).await;
    }
}

/// Optional Seal leg (feature veneers): MQTT_CHANNEL -> seal (VeneerSessionCipher) ->
/// SEALED_CHANNEL. Payload moved (same monomorphized capacity), drops unsealed on error (fail closed).
#[cfg(feature = "veneers")]
#[embassy_executor::task]
async fn seal_task(
    mut rx: Rp2350Receiver,
    tx: Rp2350Sender,
    mut stage: CryptoStage<VeneerSessionCipher>,
    handle: KeyHandle,
) {
    let mut sealed_count: u32 = 0;
    let mut dropped_count: u32 = 0;
    while let Some(mut unit) = rx.recv().await {
        let crypto_unit = CryptoUnit {
            op: CryptoOp::Seal,
            context: handle.0,
            data: unit.payload,
        };
        match stage.process(crypto_unit).await {
            Ok(sealed) => {
                unit.payload = sealed.data;
                let _ = tx.send(unit).await;
                sealed_count = sealed_count.wrapping_add(1);
            }
            Err(_e) => {
                dropped_count = dropped_count.wrapping_add(1);
            }
        }
    }
    let _ = (sealed_count, dropped_count);
}

/// Seplos -> TMS -> MQTT poll loop. Never returns. `pub(crate)` for core1-parser feature.
///
/// WP3 (delight-features, feature `mesh`): wraps `platform` in
/// `bin_mcu_platform::MeshPlatform` before handing it to `run`, so the shared poll loop also
/// forwards each reading/fault to the mesh-radio channel. Inserted HERE — not at `McuPlatform`
/// construction in `boot_seplos.rs::build_platform` — because that struct literal is built
/// directly (no `new`) in a file outside this work package's ownership; see
/// `bin_mcu_platform::MeshPlatform`'s doc comment for the full rationale. `impl_app::run<P:
/// Platform>` being generic is what makes this insertion point work: the default (non-mesh) build
/// is byte-identical (same `run(platform, cfg).await` call as before this feature existed).
#[embassy_executor::task]
pub(crate) async fn run_task(platform: Rp2350Platform, cfg: RunConfig) {
    #[cfg(not(feature = "mesh"))]
    run(platform, cfg).await;
    #[cfg(feature = "mesh")]
    {
        let mesh_tx: Rp2350MeshSender = MESH_CHANNEL.sender();
        run(bin_mcu_platform::MeshPlatform::new(platform, mesh_tx), cfg).await;
    }
}

/// Hardware watchdog gated on `POLL_HEARTBEAT` rather than fed unconditionally on a fixed timer.
///
/// **Why gate on the heartbeat.** A watchdog fed on a bare periodic timer only proves THIS task is
/// still being scheduled — nothing about whether the Seplos poll loop (`run_task`) makes forward
/// progress. Gating the feed on `POLL_HEARTBEAT` catches BOTH hang modes:
/// - **(a) Poll loop wedges:** never-completing `.await` in `impl_app::run` → stops calling
///   `platform.sleep()` → `POLL_HEARTBEAT` stops advancing → watchdog observes stall and stops feeding.
/// - **(b) Executor starvation:** any other task busy-spins and starves it → `run_task` can't run →
///   `POLL_HEARTBEAT` freezes (same as (a)) AND this task can't run either (same executor) → no feed,
///   hardware timeout triggers.
///
/// Timeout ~8s, fed on ~2s check — comfortably larger than W5500-init + DHCP window (poll loop
/// already alive well before that), and larger than check interval, so a healthy device stays fed
/// with slack. embassy-rp 0.10.0 caps `Watchdog::feed` at 0xFFFFFF µs (~16.777s on RP235x) — 8s fits.
#[embassy_executor::task]
async fn watchdog_task(mut wdt: Watchdog) {
    const TIMEOUT: embassy_time::Duration = embassy_time::Duration::from_secs(8);
    const FEED_INTERVAL: embassy_time::Duration = embassy_time::Duration::from_secs(2);

    wdt.start(TIMEOUT);
    #[cfg(feature = "defmt")]
    defmt::info!("watchdog: started (8s), petting on poll-loop liveness");

    let mut last = POLL_HEARTBEAT.load(Ordering::Relaxed);
    loop {
        embassy_time::Timer::after(FEED_INTERVAL).await;
        let now = POLL_HEARTBEAT.load(Ordering::Relaxed);
        if now != last {
            wdt.feed(TIMEOUT);
            last = now;
        } else {
            #[cfg(feature = "defmt")]
            defmt::warn!("watchdog: poll heartbeat stalled — NOT feeding (reset imminent)");
        }
    }
}

/// W5500 IP/DHCP/TCP pump.
#[embassy_executor::task]
async fn w5500_net_runner_task(mut runner: Rp2350NetRunner) {
    runner.run().await
}

/// W5500 SPI frame pump.
#[embassy_executor::task]
async fn w5500_wiznet_runner_task(runner: Rp2350WiznetRunner) {
    runner.run().await
}

/// Device-hosted MQTT broker pump. Brings up W5500 link + DHCP, logs the IP over RTT (console-less
/// board), serves concurrent clients. Socket pool, cancellation safety, and drain logic live in
/// lib_net::embassy::async_broker_pump; see types/socket_buffer_pool.rs for pool reclaim contract.
#[embassy_executor::task]
async fn broker_task(
    stack: Stack<'static>,
    broker: &'static mut BrokerCore<MAX_CLIENTS>,
    mut rx: Rp2350Receiver,
    store: &'static mut LatestStore<STORE_TOPICS, MAX_PAYLOAD>,
    rx_buf: &'static mut [u8; 512],
    out: &'static mut [u8; MAX_PAYLOAD],
) {
    // Link + DHCP progress markers (console-less board's only visibility).
    #[cfg(feature = "defmt")]
    {
        defmt::info!("w5500: init ok — waiting for link + DHCP…");
        let mut ticks: u32 = 0;
        while stack.config_v4().is_none() {
            defmt::info!(
                "net: waiting (link_up={=bool}) t={=u32}s",
                stack.is_link_up(),
                ticks
            );
            embassy_time::Timer::after(embassy_time::Duration::from_secs(2)).await;
            ticks += 2;
        }
    }

    let cfg = wait_for_config(stack).await;
    #[cfg(feature = "defmt")]
    {
        let a = cfg.address.address().octets();
        defmt::info!(
            "dhcp: acquired {=u8}.{=u8}.{=u8}.{=u8}/{=u8}",
            a[0],
            a[1],
            a[2],
            a[3],
            cfg.address.prefix_len()
        );
        if let Some(gw) = cfg.gateway {
            let g = gw.octets();
            defmt::info!(
                "net: gateway {=u8}.{=u8}.{=u8}.{=u8}",
                g[0],
                g[1],
                g[2],
                g[3]
            );
        }
        defmt::info!(
            "broker up: {=u8}.{=u8}.{=u8}.{=u8}:1883",
            a[0],
            a[1],
            a[2],
            a[3]
        );
    }
    #[cfg(not(feature = "defmt"))]
    let _ = cfg;

    // IPv6 dual-stack (MIL-STD-3071 DDS/RTPS). Render link-local + global (if advertised).
    #[cfg(feature = "defmt")]
    {
        let ll = link_local_v6(crate::boot_w5500::W5500_MAC).segments();
        defmt::info!(
            "net: ipv6 fe80::{=u16:x}:{=u16:x}:{=u16:x}:{=u16:x}",
            ll[4],
            ll[5],
            ll[6],
            ll[7]
        );
        if let Some(v6) = stack.config_v6() {
            let g = v6.address.address().segments();
            defmt::info!(
                "net: ipv6 global {=u16:x}:{=u16:x}:{=u16:x}:{=u16:x}:{=u16:x}:{=u16:x}:{=u16:x}:{=u16:x}/{=u8}",
                g[0], g[1], g[2], g[3], g[4], g[5], g[6], g[7],
                v6.address.prefix_len()
            );
        }
    }

    // Socket pool + accept source. Reserve free pair, build socket, accept on :1883. Drop guard
    // frees on cancel (declaration order: sock drops first, releasing borrow; reservation drops
    // second, freeing the pair). See types/socket_buffer_pool.rs for the contract.
    let pool: &'static SocketBufferPool = &SOCKET_POOL;
    let accept_source = move || async move {
        let reservation = match pool.reserve() {
            Some(r) => r,
            None => {
                embassy_time::Timer::after(embassy_time::Duration::from_millis(50)).await;
                return None;
            }
        };
        let (buf_rx, buf_tx) = unsafe { reservation.buffers() };
        let mut sock = TcpSocket::new(stack, buf_rx, buf_tx);
        sock.set_timeout(Some(embassy_time::Duration::from_secs(30)));
        match sock.accept(1883).await {
            Ok(()) => {
                // === ADR 0013 transport-security interposition point (feature `tls`, default OFF).
                // This is where the TLS stage sits: between the accepted TcpSocket and the
                // `PooledSocket` handed to `async_broker_pump`. Today it is a fail-closed STUB —
                // `interpose_tls`'s success type is uninhabited, so with `tls` ON this arm can
                // ONLY refuse (the broker then serves nobody, which is the intended behaviour of a
                // security stub), and with `tls` OFF — the default, and every shipping build —
                // not one instruction of it is compiled and the plaintext path below is unchanged.
                // See `boot_tls.rs`.
                #[cfg(feature = "tls")]
                {
                    match boot_tls::interpose_tls(&mut sock).await {
                        // Uninhabited: `Infallible` has no values, so there is no plaintext
                        // fallback expressible here — not even by accident, and not by a later
                        // "just return Ok for now" edit. When WP-6 gives the stage a real success
                        // type this arm stops compiling, deliberately, so the transition is caught
                        // by the compiler rather than by review.
                        Ok(never) => match never {},
                        Err(refusal) => {
                            #[cfg(feature = "defmt")]
                            defmt::error!(
                                "tls: connection REFUSED (fail-closed) — {=str}",
                                refusal.reason()
                            );
                            #[cfg(not(feature = "defmt"))]
                            let _ = refusal;
                            // The pump's existing `Either3::First(None)` arm handles this; no new
                            // error path in `lib-net` (ADR 0013 §6).
                            None
                        }
                    }
                }
                #[cfg(not(feature = "tls"))]
                {
                    Some(PooledSocket::new(sock, reservation))
                }
            }
            Err(_) => None,
        }
    };

    // Event sink: broker lifecycle -> RTT lines (no-op if !defmt).
    let events = |ev: BrokerEvent| {
        #[cfg(feature = "defmt")]
        match ev {
            BrokerEvent::BrokerUp => defmt::info!("mqtt: broker up :1883"),
            BrokerEvent::ClientConnect => defmt::info!("mqtt: client connect"),
            BrokerEvent::ClientClose => defmt::info!("mqtt: client close"),
            BrokerEvent::TopicPublish => defmt::info!("mqtt: publish"),
            _ => {}
        }
        #[cfg(not(feature = "defmt"))]
        let _ = ev;
    };

    // Run the generic N-client pump (never returns for static channel).
    async_broker_pump::<PooledSocket, _, _, _, MAX_CLIENTS, STORE_TOPICS, MAX_PAYLOAD>(
        accept_source,
        broker,
        &mut rx,
        store,
        &mut rx_buf[..],
        &mut out[..],
        events,
    )
    .await;
}

/// Profiler reporting (feature profile only): span measurements over RTT, every 10 seconds.
#[cfg(feature = "profile")]
#[embassy_executor::task]
async fn profiler_task() {
    let clock = EmbassyClock::new();
    let sink = impl_profile_sink::DefmtProfileSink;

    embassy_time::Timer::after(embassy_time::Duration::from_secs(5)).await;

    loop {
        defmt::info!("[profiler] emitting report...");
        lib_profile::report(&PROFILE_REGISTRY, &sink);
        embassy_time::Timer::after(embassy_time::Duration::from_secs(10)).await;
    }
}

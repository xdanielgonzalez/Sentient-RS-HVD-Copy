//! Bring-up helpers for the W5500 Ethernet + device-hosted broker leg of `main` (Story S8 #359,
//! EPIC #350): SPI0 + W5500 bring-up ([`build_w5500`]) and net/broker task spawn
//! ([`spawn_net_and_broker`]). Always compiled (not feature-gated) since the board has no console
//! and RTT-visible link-up/DHCP is the point.
//!
//! `STACK_SOCKETS` sizes the WHOLE smoltcp socket table (embassy-net's DHCPv4 + DNS each claim a
//! slot too) — a previous `MAX_CLIENTS + 1` silently capped live clients; fixed at `MAX_CLIENTS + 4`.

use embassy_executor::Spawner;
use embassy_net::{Config as NetConfig, DhcpConfig, StackResources};
use embassy_net_wiznet::{InitError, State};
use embassy_rp::gpio::{Input, Level, Output, Pull};
use embassy_rp::peripherals::{
    DMA_CH0, DMA_CH1, PIN_10, PIN_11, PIN_20, PIN_22, PIN_23, PIN_26, SPI0,
};
use embassy_rp::spi::{self, Spi};
use embassy_rp::Peri;
use embassy_time::Delay;
use embedded_hal::spi::ErrorType;
use embedded_hal_bus::spi::ExclusiveDevice;
use lib_mapping::sink::MAX_PAYLOAD;
use lib_net::embassy::build_w5500_stack;
use lib_net::embassy::pump::LatestStore;
use lib_net::link::BrokerCore;

use crate::{
    Rp2350Receiver, Rp2350SpiDevice, Rp2350W5500Parts, STACK_SOCKETS, W5500_RX_QUEUE_SIZE,
    W5500_TX_QUEUE_SIZE,
};

/// DHCP hostname (Option 12) the device advertises on the network — the console-less device's
/// on-wire identity. Injected at **flash/build time** via the `DEVICE_HOSTNAME` env var (the justfile
/// `mcu-flash-device` recipe passes it through the build); a plain `cargo build` with no override
/// falls back to `meteor-pico`. `option_env!` bakes it in at compile time, so there is no runtime
/// config surface on the board.
pub(crate) const HOSTNAME: &str = match option_env!("DEVICE_HOSTNAME") {
    Some(h) => h,
    None => "meteor-pico",
};

/// Locally-administered placeholder MAC the W5500 (and DHCP/ARP/NDP) report — production: a
/// device-provisioned MAC. Hoisted to a `pub(crate)` const so the stack build here and the IPv6
/// link-local render in `main.rs`'s `broker_task` derive the `fe80::` address from ONE source of
/// truth (they must agree — the link-local is a pure function of this MAC).
///
/// TODO(ADR 0013): per-unit identity — fixed, will collide across units. Both the RTPS
/// `GuidPrefix` (`crate::spdp::SpdpAnnounce::new`) AND the `fe80::` link-local (`main.rs`'s
/// `broker_task`) are derived from THIS one MAC, so two units sharing this literal collide
/// TWICE: identical link-local addresses on the same segment (one unit's traffic silently
/// shadows/black-holes the other's — duplicate-address detection, if implemented, would fault
/// both), and identical RTPS GUID prefixes (a DDS peer treats both participants as the SAME
/// participant, merging/dropping discovery and DATA traffic from whichever loses the race).
/// Redesign in flight: derive per-unit MAC from the device certificate via the `TrustStore` seam
/// (`docs/decisions/0013-transport-security-trust-store.md`). Do not fix here — a wrong fix now
/// would conflict with that work item.
pub(crate) const W5500_MAC: [u8; 6] = [0x02, 0x00, 0x00, 0xaa, 0xbb, 0xcc];

/// The error [`build_w5500`] returns on a W5500 that never responds on SPI (chip absent /
/// mis-wired) — `main` logs it (feature `defmt`) and leaves the rest of the device running rather
/// than panicking the image over a leg that is not the primary mission.
///
/// Named via `embedded_hal::spi::ErrorType` (NOT `embedded_hal_async::spi::SpiDevice`, the trait
/// `Rp2350SpiDevice` is actually bounded by at the `build_w5500_stack` call below): `SpiDevice`
/// only *inherits* the `Error` associated type from its `ErrorType` supertrait, and Rust's
/// `<T as Trait>::Assoc` path syntax requires naming the trait that directly DECLARES `Assoc`.
pub(crate) type Rp2350W5500Error = InitError<<Rp2350SpiDevice as ErrorType>::Error>;

/// Bring the on-board W5500 up over SPI0 + `embassy-net`/DHCPv4 and assemble the `embassy_net::Stack`
/// over it. MAC is a locally-administered placeholder (production: device-provisioned).
///
/// Board pin map (WP1, delight-features, 2026-07-25: Adafruit Feather RP2350 pinout, verified
/// against four independent primary sources cross-checked against each other — see the
/// `feather-rp2350-pinout-ground-truth` research note; replaces the Pico 2's W5500-EVB-Pico2
/// reference pinout this crate used before the board move):
/// - SPI0: CLK=GP22, MOSI=GP23, MISO=GP20, CS=GP10
/// - W5500 INT: GP11 (pull-up, active-low)
/// - W5500 RST: GP26 (active-low reset)
#[allow(clippy::too_many_arguments)]
pub(crate) async fn build_w5500(
    spi0: Peri<'static, SPI0>,
    clk: Peri<'static, PIN_22>,
    mosi: Peri<'static, PIN_23>,
    miso: Peri<'static, PIN_20>,
    tx_dma: Peri<'static, DMA_CH0>,
    rx_dma: Peri<'static, DMA_CH1>,
    cs_pin: Peri<'static, PIN_10>,
    // INT/RESET must NOT be swapped — on the earlier Pico 2 pinout these two were once mixed up
    // (INT wired to the pad this crate treated as RESET): with INT sitting on a pin that's pulled
    // up and never asserts low, `embassy-net-wiznet`'s `Runner::run()` — which gates all RX
    // draining on `int.wait_for_low()` — never drained received frames, so DHCPOFFER sat in the
    // W5500 buffer and `config_v4()` never resolved even though init + link-up (which don't use
    // INT) succeeded. On the Feather RP2350 pinout: INT = GP11, RESET = GP26.
    int_pin: Peri<'static, PIN_11>,
    reset_pin: Peri<'static, PIN_26>,
) -> Result<Rp2350W5500Parts, Rp2350W5500Error> {
    // W5500 SPI clock. embassy-rp's `Config::default()` is 1 MHz (works for register I/O but is
    // needlessly slow). The integrated WIZnet EVB-Pico2 example uses 50 MHz on short PCB traces —
    // but at 50 MHz over jumper wires the RX-length read corrupts (garbage frame length → previously
    // a slice panic in embassy-net-wiznet device.rs:216). That panic is now GUARDED rather than
    // merely avoided-by-frequency: `crates/bin/mcu-rp2350/vendor/embassy-net-wiznet` vendors 0.3.0
    // with `read_frame`'s header validated before use (underflow-checked, buffer-capped — mirrors
    // upstream PR #5706, unreleased; see that vendor dir's PATCHES.md), so a corrupted header now
    // drops the frame (`Ok(0)`) instead of panicking the image. 16 MHz stays as defense-in-depth —
    // lowering the corruption RATE is still worth it even though a corrupted header can no longer
    // reboot the board — not as the sole mitigation anymore. 16 MHz is a robust middle ground: fast
    // enough for DHCP/telemetry throughput, slow enough to substantially reduce discrete-module
    // wiring corruption. Raise toward 50 MHz only on a board with clean, short SPI routing.
    let mut spi_cfg = spi::Config::default();
    spi_cfg.frequency = 16_000_000;
    let spi = Spi::new(spi0, clk, mosi, miso, tx_dma, rx_dma, crate::Irqs, spi_cfg);
    let cs = Output::new(cs_pin, Level::High);
    let int = Input::new(int_pin, Pull::Up);
    let reset = Output::new(reset_pin, Level::High);

    // Wrap the SPI + CS into an `embedded_hal_async::spi::SpiDevice` that the WIZnet driver
    // expects. embedded-hal-bus's ExclusiveDevice handles the CS line toggle per transaction.
    let spi_dev = ExclusiveDevice::new(spi, cs, Delay);

    // Initialize W5500 state and stack resources from StaticCell storage.
    let wiznet_state = crate::W5500_STATE.init(State::new());
    let stack_resources = crate::W5500_STACK_RESOURCES.init(StackResources::new());

    // DHCPv4 config carrying the build-time hostname (Option 12). `String::try_from` fails CLOSED to
    // `None` (no Option 12 sent) if a `DEVICE_HOSTNAME` override exceeds embassy-net's
    // `MAX_HOSTNAME_LEN` (32) — never advertise a silently-truncated identity. The target field type
    // (`Option<heapless::String<32>>`) drives the capacity inference, so we don't name the 32 here.
    let mut dhcp = DhcpConfig::default();
    dhcp.hostname = heapless::String::try_from(HOSTNAME).ok();
    #[cfg(feature = "defmt")]
    defmt::info!("net: hostname {=str}", HOSTNAME);

    build_w5500_stack::<_, _, _, W5500_RX_QUEUE_SIZE, W5500_TX_QUEUE_SIZE, STACK_SOCKETS>(
        W5500_MAC,
        wiznet_state,
        stack_resources,
        spi_dev,
        int,
        reset,
        NetConfig::dhcpv4(dhcp),
        // TODO(ADR 0013): per-unit identity — fixed, will collide across units. This seeds
        // embassy-net's internal PRNG (ephemeral source ports, IPv6 DAD nonces, etc.); every unit
        // sharing this literal draws the SAME "random" sequence, so two units on one network can
        // pick identical ephemeral ports / DAD timings at the same moment — a low-probability but
        // real collision mode this seed makes fully deterministic and reproducible across units.
        // Redesign in flight: derive per-unit seed from the device certificate via the
        // `TrustStore` seam (`docs/decisions/0013-transport-security-trust-store.md`). Do not fix
        // here — a wrong fix now would conflict with that work item.
        42, // random_seed (placeholder; production: inject from device secrets)
    )
    .await
}

/// Initialize the MQTT broker core (`MAX_CLIENTS` concurrent client slots) + latest-value store +
/// per-read scratch, then spawn the net/wiznet/broker task trio: the IP/DHCP/TCP pump
/// ([`crate::w5500_net_runner_task`]), the SPI frame pump ([`crate::w5500_wiznet_runner_task`]),
/// and the broker pump ([`crate::broker_task`] — DHCP-wait + IP-announce + idle; see its own doc
/// comment in `main.rs`), which consumes `drain_rx`.
pub(crate) fn spawn_net_and_broker(
    spawner: Spawner,
    w5500_parts: Rp2350W5500Parts,
    drain_rx: Rp2350Receiver,
) {
    let broker = crate::BROKER_CORE.init(BrokerCore::new());
    let store =
        crate::LATEST_STORE.init(LatestStore::<{ crate::STORE_TOPICS }, MAX_PAYLOAD>::new());
    let rx_buf = crate::W5500_RX_BUF.init([0u8; 512]);
    let payload_scratch: &'static mut [u8; MAX_PAYLOAD] =
        crate::PAYLOAD_SCRATCH.init([0u8; MAX_PAYLOAD]);

    spawner.spawn(
        crate::w5500_net_runner_task(w5500_parts.net_runner)
            .expect("failed to create w5500_net_runner_task token (pool exhausted)"),
    );
    spawner.spawn(
        crate::w5500_wiznet_runner_task(w5500_parts.wiznet_runner)
            .expect("failed to create w5500_wiznet_runner_task token (pool exhausted)"),
    );
    spawner.spawn(
        crate::broker_task(
            w5500_parts.stack,
            broker,
            drain_rx,
            store,
            rx_buf,
            payload_scratch,
        )
        .expect("failed to create broker_task token (pool exhausted)"),
    );
}

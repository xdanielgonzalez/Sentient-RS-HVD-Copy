# crates/bin/mcu-rp2350

RP2350 (Cortex-M33) MCU shipping binary — replaces the retired Zephyr MCU path (`bin/zephyr`).
Story S1 #352 shipped an NS-world-only skeleton; **story S4 #355 (this update) adds the
TrustZone-M S/NS split**: `s` (secure world) + `abi` (the frozen NS<->S ABI) join `ns` (non-secure
world) as workspace members.

**THIS IS A DEMO SCAFFOLD, NOT PRODUCTION.** Claims are limited to signed boot / load-time
integrity / IP protection vs non-physical attackers. NO FIPS, NO attestation, NO anti-tamper.
RP2350 A4 stepping only, unverified on real hardware or under Renode (this story's gate is a
`cargo build` that links; see "Known gaps" below for what that does and does not prove).

Own excluded Cargo workspace (own `Cargo.lock`, own `thumbv8m.main-none-eabihf` target/toolchain
pins), mirroring `hil/pico-seplos-emu`'s convention — never `cargo build --workspace` from the repo
root.

Build: `cd crates/bin/mcu-rp2350 && cargo build --release`

**Claims:** UART0 Seplos transport + monotonic clock + a bounded MQTT-unit channel wired into
`bin_mcu_platform::McuPlatform`, running the shared `impl_app::run` poll loop under
embassy-executor (`ns`); **story S8 #359** additionally brings up the on-board W5500 over
`embassy-net`/DHCPv4 and always compiles two publish legs off that same Seplos telemetry: DDS/RTPS
over UDP/IPv6 multicast (SPDP participant announce + a `DcMeasurementUpdate` DATA publish, domain
0) — the MIL-STD-3071 TMS compliance path this networking exists to satisfy — and a device-**hosted**
MQTT 3.1.1 broker on port 1883 (up to `MAX_CLIENTS` = 3 concurrent clients), a pragmatic integration
surface for whatever consumes the telemetry (an operator tool, a test harness, or a bridge onward to
something like AWS IoT Greengrass) rather than a compliance requirement in its own right — link
up/down, DHCP acquired/lost, and both publish legs' lifecycle events all visible over RTT (feature
`defmt`). **Non-claims:** A4-stepping RP2350A only, unverified on hardware; the W5500 pinout is an
assumed-not-HIL-verified hardware wiring (see "Pinout" below); the DDS leg publishes a synthetic
reading, not live Seplos telemetry yet (see "DDS/RTPS" below); the MQTT leg is QoS 0 only, plaintext,
unauthenticated; the device MAC and DHCP identity are fixed placeholders (TODO before multi-board
deployments).

## TrustZone-M S/NS split (story S4 #355)

Three workspace members now build the split image:

* **`abi`** — `rp2350-abi`, a `no_std`, target-independent (host-testable) lib crate: the frozen
  `sv_*` error codes ([`SvError`]), custody codes ([`SvCustody`]), `SV_MAX_PAYLOAD`, and const
  mapping fns to/from `lib_security::{SecurityError, KeyCustody}`. `cargo test -p rp2350-abi
  --target <host-triple>` runs its mapping tests on the host, no emulator needed.
* **`s`** — `rp2350-s`, the secure-world binary. Boots first (owns reset — see "Known gaps" below
  on the still-open boot-ROM/IMAGE_DEF question), configures the SAU (`s/src/sau.rs`), brings up
  the `sv_*` veneer service (`s/src/service.rs`, backed by `lib-security`'s
  `SoftwareSessionStore`), then hands off to the NS image via `bxns` (`s/src/ns_jump.rs`). No
  executor — a bare `cortex-m-rt` binary; the veneers are one-shot synchronous CMSE entries.
* **`ns`** — unchanged shipping skeleton from story S1, growing an optional `veneers` feature
  (Agent B's territory) that rides the frozen ABI below.
* **`veneers/veneer.{c,h}`** — the actual `cmse_nonsecure_entry` C shim, compiled `-mcmse` by
  `s/build.rs`. Not a Cargo package (plain C, compiled directly into the `rp2350-s` binary).

### Memory split

Board (WP1, delight-features, 2026-07-25): **Adafruit Feather RP2350, 8 MB flash** — moved off the
Pico 2 (4 MB flash) this split was originally derived against. Same silicon
(`thumbv8m.main-none-eabihf`, embassy-rp, the TrustZone-M S/NS split all carry over unchanged);
only the flash window widened. RAM is **unchanged** by the board move — flash was never this
target's binding constraint, RAM was (static RAM usage went 76.3% -> 20.2% via the `LATEST_STORE`
right-size, tracked separately from this port).

| Region | Range | Notes |
|---|---|---|
| S flash | `0x1000_0000`..`0x1002_0000` (128 KiB) | `s/memory.x` |
| — NSC veneer subregion | `0x1001_F000`..`0x1002_0000` (last 4 KiB of S flash) | SAU region 2, `RLAR.NSC=1`; holds `.gnu.sgstubs` |
| NS flash | `0x1002_0000`..`0x1080_0000` (8064 KiB) | `ns/memory.x` (8 MB Feather window) |
| S RAM | `0x2000_0000`..`0x2001_0000` (64 KiB) | `s/memory.x` |
| NS RAM | `0x2001_0000`..`0x2008_0000` (448 KiB) | `ns/memory.x` — unchanged by the board move |

Full derivation and rationale: `s/memory.x`'s header comment and `s/src/mem_layout.rs` (the
Rust-side constants `sau.rs`/`ns_jump.rs` consume, with compile-time consistency asserts tying the
table together).

**Known headroom, not claimed here:** the RP2350's SRAM is actually one contiguous 520 KiB region
(`0x2000_0000`..`0x2008_2000` per the datasheet) — this split's arithmetic still conservatively
claims only 512 KiB, leaving `0x2008_0000`..`0x2008_2000` (8 KiB) real, present SRAM unclaimed by
either world. Left alone deliberately: claiming it would move the S/NS split boundary, which is out
of scope for a pin/flash-window port. See `s/src/mem_layout.rs`'s `NS_RAM_END` doc comment.

### Veneer ABI

Four `cmse_nonsecure_entry` C functions are the only way NS code reaches the secure world. See
`veneers/veneer.h` for the full documented signatures and `abi/src/lib.rs` for the Rust-side
mirror (error/custody codes + mapping fns).

**Correction to the originally-specified `sv_seal`/`sv_open` signatures** (verified against the
compiler, not guessed): the story brief specified 6 plain arguments each
(`handle, aad, aad_len, buf, cap, len`). Building that exactly as given fails —
`arm-none-eabi-gcc 13.2.1` rejects it: `'cmse_nonsecure_entry' attribute not available to functions
with arguments passed on the stack` (ARMv8-M requires every `cmse_nonsecure_entry` argument to fit
in AAPCS registers `r0`-`r3`; 6 arguments spill 2 of them to the stack, which CMSE forbids). Fixed
by bundling the four arguments beyond `handle` into one NS-pointer-passed struct:

```c
typedef struct {
    const uint8_t *aad;
    uint32_t aad_len;
    uint8_t *buf;
    uint32_t cap;
    uint32_t len;
} sv_crypto_args_t;

int32_t sv_seal(uint32_t handle, sv_crypto_args_t *args);
int32_t sv_open(uint32_t handle, sv_crypto_args_t *args);
```

`sv_session_establish` (0 args) and `sv_profile` (2 pointer args) already fit in 4 registers and
are **unchanged** from the original brief. `args` itself is validated with
`cmse_check_address_range` before any field is read; `args->aad`/`args->buf` are then validated
again (validating the struct's own memory says nothing about what its pointer fields point to).

Error codes (`abi/src/types/sv_error.rs::SvError`, mirrored in `veneers/veneer.h`):

| Code | Name | Meaning |
|---|---|---|
| -1 | `SV_ERR_NOT_IMPLEMENTED` | Operation not implemented by this build's backend |
| -2 | `SV_ERR_KEY_NOT_FOUND` | Unknown session handle |
| -3 | `SV_ERR_VERIFICATION_FAILED` | `sv_open` authentication failed |
| -4 | `SV_ERR_BUFFER_TOO_SMALL` | Output/framing doesn't fit the caller's buffer |
| -5 | `SV_ERR_CAPACITY` | Input exceeds this scaffold's fixed internal scratch |
| -6 | `SV_ERR_BAD_POINTER` | An NS pointer failed `cmse_check_address_range` |
| -7 | `SV_ERR_BACKEND` | Underlying crypto/service backend failure |

Custody codes `sv_profile` reports (`abi/src/types/sv_custody.rs::SvCustody`): `0` =
`SoftwareMemory`, `1` = `SecureWorld`, `2` = `SecureElement`. This scaffold's build only ever
reports `SoftwareMemory` (no `trustzone`/`secure-element` `lib-security` feature is enabled).

### Build order + the veneer implib artifact

1. `s` builds first: compiles `veneers/veneer.c` (`-mcmse`), links `rp2350-s`, and — AT THAT SAME
   link step — emits the CMSE import library (`--cmse-implib --out-implib=<path>`, the standard
   two-stage ARMv8-M CMSE workflow: a small relocatable object containing only the
   NS-callable veneer trampoline symbols).
2. **Frozen artifact contract for `ns`:** stable path
   `crates/bin/mcu-rp2350/target/veneer-implib/libveneer_implib.o` (workspace-relative, not under any
   per-profile `OUT_DIR`) and env var `SENTIENT_VENEER_IMPLIB`. `s`'s own build script cannot set an
   OS environment variable in `ns`'s separate `cargo`/`build.rs` process — `ns/build.rs` should read
   `SENTIENT_VENEER_IMPLIB` if set, else fall back to the same stable path computed from its own
   `CARGO_MANIFEST_DIR`.
3. `ns --features veneers` builds second, linking against that implib to resolve `sv_*` calls.

**Mechanism proven by actually building `rp2350-s`**, not assumed from documentation — three real
linker errors were hit and fixed in sequence (see `s/build.rs`'s comments at each fix):
archived-vs-direct-object CMSE symbol recognition (`rust-lld` rejected `cmse_nonsecure_entry`
symbols pulled from an `ar` static archive; fixed by compiling straight to a `.o` and linking it
directly, no archive), a missing multilib `libgcc.a` link (`cmse_check_address_range` is a real
function GCC's ARM CMSE support ships per-multilib, not a compiler-inlined intrinsic; resolved via
`gcc -print-libgcc-file-name`, not a hand-guessed path), and an unplaced `.gnu.sgstubs` output
section (plain `cortex-m-rt`/`link.x` has no rule for it; fixed with a generic
`--section-start=.gnu.sgstubs=<NSC_BASE>` linker flag). The target's default linker for
`thumbv8m.main-none-eabihf` (no custom `linker =` in `.cargo/config.toml`) is `rust-lld` invoked
directly (`-flavor gnu`), NOT a gcc driver — so linker flags here have no `-Wl,` prefix (that
stripping is a gcc-frontend convention that doesn't apply to a direct linker invocation).

### Known gaps (documented, not silently assumed)

* **RP2350 boot-ROM IMAGE_DEF.** The boot ROM requires a valid IMAGE_DEF metadata block
  (`.start_block`) in the first 4 KiB of whatever image it boots from flash base `0x1000_0000`.
  Before this split, `ns`'s `embassy-rp` dependency auto-generated that block just by being linked
  in (its own `lib.rs` macro, feature `_rp235x`). `s` — now the image at flash base `0x1000_0000` —
  depends on neither `embassy-rp` (no executor, no HAL) nor a hand-authored IMAGE_DEF (asserting its
  exact binary layout from memory, unverified against the datasheet, would be inventing a fact).
  Net effect: `rp2350-s` is proven to compile and link correctly (SAU config / veneer service /
  `bxns` handoff), NOT yet proven boot-ROM-loadable on real hardware or under Renode. Open item for
  the hardware bring-up ledger, not this story's gate.
- **DMA bypasses the SAU.** The SAU only gates CPU-core accesses; a real product needs an
  additional RP2350 bus-level access-control mechanism (the datasheet names an `ACCESSCTRL`
  peripheral) to stop a compromised/misbehaving NS-side DMA descriptor from reaching S memory. Not
  wired up here — `s/src/sau.rs`'s doc comment names this by concept only, asserting no register
  layout unverified against the datasheet. TODO(#355).
- **Entropy.** `s`'s session-key RNG is seeded from the Cortex-M `DWT` cycle counter — explicitly
  NOT a hardware TRNG (documented in `s/src/service.rs`). TODO(#355): wire an actual entropy source
  before any real key material depends on this build.
- **Payload/AAD chunking.** `SV_MAX_PAYLOAD` (4096 B) and the veneer's internal 64 B AAD scratch are
  hard per-call caps; chunking a larger input across multiple calls is TODO(#355), not implemented.

## W5500 Ethernet bring-up (story S8 #359)

`ns` brings up the on-board WIZnet W5500 over SPI0 + `embassy-net`/DHCPv4 — **always compiled**
(unlike `veneers`/`core1-parser`, this is not feature-gated: the board has no console, so proving
link-up/DHCP is observable over RTT is this story's whole point). See `ns/src/boot_w5500.rs`'s
module doc for the bring-up implementation and `main.rs`'s `broker_task` for the DHCP-lifecycle
logging; this section covers the pinout, the DHCP flow, and how to watch it.

**Claims:** the W5500 driver (`embassy-net-wiznet`) + `embassy-net`'s IP stack + DHCPv4 client come
up as three tasks (`w5500_wiznet_runner_task`, `w5500_net_runner_task`, `broker_task`), spawned
once from `main.rs` alongside (never blocking) the Seplos/UART poll+drain pair — a W5500 that never
links, or isn't physically present yet, does not stall telemetry. **Non-claims:** the MAC address is
a fixed, non-unique, locally-administered constant (TODO(#359): a real per-device-provisioned MAC
before more than one board shares an L2 segment). (The device-hosted MQTT broker that also rides
this stack is covered in its own section below, with its own Claims/Non-claims — it is NOT a
"no MQTT/TCP socket yet" placeholder.)

### Pinout — Adafruit Feather RP2350 (WP1, delight-features, 2026-07-25)

**Board:** Adafruit Feather RP2350 (8 MB flash), replacing the earlier Raspberry Pi Pico 2 (4 MB
flash) this crate originally targeted. Verified against **four independent primary sources**
cross-checked against each other (Adafruit LEARN pinouts/power-management pages fetched raw —
not the AI-summarized version, which was internally contradictory — the Adafruit schematic, the
Adafruit `prettypins` silkscreen diagram, and the RP2350 datasheet's own GPIO Bank 0 function
table): see the `feather-rp2350-pinout-ground-truth` research note for the full methodology and
citations.

| Signal | GPIO | Notes |
|---|---|---|
| W5500 SPI0 SCK | GP22 | |
| W5500 SPI0 MOSI | GP23 | |
| W5500 SPI0 MISO | GP20 | |
| W5500 SPI0 CSn | GP10 | plain GPIO, manually toggled by `embedded-hal-bus`'s `ExclusiveDevice` — not one of `embassy_rp::spi::Spi`'s own hardware CS pins |
| W5500 `RSTn` | GP26 | active-low, idles high |
| W5500 `INTn` | GP11 | active-low, open-drain — configured with an internal pull-up |
| Seplos UART0 TX | GP0 | unchanged from the Pico 2 layout |
| Seplos UART0 RX | GP1 | unchanged from the Pico 2 layout |
| Seplos RS-485 DE | GP29 | **moved off GP2** — GP2 is this board's fixed I2C1 SDA, reserved for the OLED FeatherWing (see reserved rows below); the Pico 2 layout wrongly used GP2 for DE |

**Reserved — NOT wired up by this work package, do not reassign:**

| Signal | GPIO | Notes |
|---|---|---|
| Heltec UART1 TX | GP24 | wired by `boot_mesh.rs::build_mesh_transport` (feature `mesh`) — **the same physical pin as "Button A" below** (feature `ui`); `mesh` and `ui` are mutually exclusive by design, enforced by a `compile_error!` in `ns/src/main.rs`, not left to fail as a move-semantics error |
| Heltec UART1 RX | GP25 | wired by `boot_mesh.rs::build_mesh_transport` (feature `mesh`) — same physical pin as "Button B" below, same `mesh`/`ui` conflict |
| I2C1 SDA | GP2 | fixed by the board (main header + STEMMA QT share this bus); reserved for the OLED FeatherWing (feature `ui`) — singly owned, not shared with the INA3221 (see I2C0 rows below) |
| I2C1 SCL | GP3 | fixed by the board; reserved for the OLED FeatherWing (feature `ui`) |
| I2C0 SDA | GP4 | the bench's actual INA3221 wiring; reserved for the INA3221 power-telemetry work package (feature `power`) — a DIFFERENT peripheral than I2C1 above, singly owned, not shared |
| I2C0 SCL | GP5 | the bench's actual INA3221 wiring; reserved for the INA3221 power-telemetry work package (feature `power`) |
| Button A | GP24 | fixed by the OLED FeatherWing; wired by `boot_ui.rs::button_task` (feature `ui`) — **the same physical pin as "Heltec UART1 TX" above** (feature `mesh`); `mesh` and `ui` cannot both be enabled at once — `ns/src/main.rs` has a `compile_error!` naming this exact conflict if both features are turned on together |
| Button B | GP25 | fixed by the OLED FeatherWing (only button with an onboard pull-up); wired by `boot_ui.rs::button_task` — same physical pin as "Heltec UART1 RX" above, same `mesh`/`ui` conflict |
| Button C | GP27 | fixed by the OLED FeatherWing; wired by `boot_ui.rs::button_task` (feature `ui`) |

All nine GPIO numbers wired up in this work package (the "Pinout" table above: 3 Seplos pins + 6
W5500 pins) are named in exactly one place each (`ns/src/boot_seplos.rs`'s and
`ns/src/boot_w5500.rs`'s function signatures, as typed
`Peri<'static, PIN_n>` parameters) with the call sites in `ns/src/main.rs` passing the matching
`p.PIN_n` field — a mismatch between signature and call site is a compile error, not a silent
bench-wiring bug.

### DHCP / link-lifecycle flow

Three tasks, spawned once by `boot_w5500::spawn_net_and_broker` (`main.rs`):

1. **`w5500_wiznet_runner_task`** — the W5500 driver's background `Runner` (SPI frame RX/TX pump +
   link-state poll over `INTn`). Never returns.
2. **`w5500_net_runner_task`** — `embassy-net`'s own IP-stack `Runner` (ARP, the DHCPv4 client, the
   `smoltcp` interface poll). Never returns.
3. **`broker_task`** — waits for DHCP (`lib_net::embassy::wait_for_config`), logging `link_up`
   status and the current wait duration over RTT (feature `defmt`) while it does, then announces
   the acquired IP and runs the broker pump forever. There is no separate link/config watcher task
   in this crate — `broker_task` itself is the only consumer of the wait.

If `build_w5500` returns an error (chip not present/not responding on SPI0 — the expected state
before the physical HIL wiring exists), `main` logs `W5500 init failed` and returns from `main`
**without** spawning any of the three tasks, rather than panicking the whole image over a leg that
is not the primary Seplos->TMS->MQTT-unit mission.

### Watching it over RTT

The board has no console — `defmt`/RTT is the only way to observe any of this. Build with
`--features defmt` and see `just mcu-rp2350-rtt` for the full manual bench attach flow (OpenOCD over
the bench's linuxgpiod SWD adapter + `defmt-print`). With `defmt` **off** (the default), all of the
above still runs identically — it is just silent; nothing in `boot_w5500.rs` behaves differently, no
log call partially fires. Expect, over RTT, as the bench Feather RP2350's W5500 links and DHCPs:

```
w5500: link up
dhcp: acquired 10.0.1.42/24 gw 10.0.1.1
```

and, on a cable pull or DHCP lease loss:

```
w5500: link down
dhcp: lost
```

### Version pins (verified against the crates.io index + embassy-rs/embassy's own examples)

| Crate | Version | Why |
|---|---|---|
| `embassy-net-wiznet` | `0.3.0` | declares `embassy-time` req `^0.5.1` — the EXACT version this workspace already pins (no second `embassy-time-driver`/its `links` key entering the graph) |
| `embassy-net` | `0.9.1` | same exact `^0.5.1` embassy-time req; also `heapless` req `^0.9`, matching this workspace's own `heapless = "0.9"` pin (one resolved `heapless`, unlike `lib-net`'s own embassy-net 0.6.0 spike which needs a second `heapless-embassy-net` 0.8) |
| `embassy-futures` | `0.1.2` | exactly what `embassy-net-wiznet` 0.3.0 itself requires (`^0.1.2`) — used for `select`/`Either` in `impl_buffered_uart_adapter.rs`/`boot_mesh.rs`/`boot_ui.rs`, and for the accept/read/channel `select3` inside `lib_net::embassy::pump::async_broker_pump` that `broker_task` drives |
| `embedded-hal-bus` | `0.1` (`features = ["async"]`) | the `ExclusiveDevice` SPI wrapper — mirrors embassy-rs/embassy's own `examples/rp/src/bin/ethernet_w5500_tcp_client.rs` (fetched from the `main` branch to confirm), which pins this exact crate/version/feature for this exact role |

**Stale, flagged rather than smoothed over:** this paragraph used to describe `embassy-net-driver-channel`/
`embassy-net`'s `embassy-sync` req of `^0.8.0` as a *second*, different-major `embassy-sync` from
this workspace's own pin (`0.6.2` at the time). That is no longer the case: this workspace's own
`embassy-sync` pin was itself bumped `0.6.2` -> `0.8.0` (story S8 #359, to match `lib-net`'s own
`embassy-sync` pin, also `0.8.0` — see `crates/lib-net/Cargo.toml`), so exactly ONE `embassy-sync`
now resolves across the whole graph, not two side by side. `embassy-sync` carries no `links` key
either way (verified), so this was never a hard conflict, just a duplicate-crate cost that the
bump also happened to remove.

Deliberately **not** enabling either crate's own `defmt` feature: both declare a `defmt` dep req of
`^1.0.1` (verified in the same index fetch). That was a hard conflict when this pin was chosen — the
workspace then pinned `defmt = "0.3"`, and defmt 1.x's `links = "defmt"` key made the two
uncombinable on top of the semver break. **Stale, flagged rather than smoothed over:** the workspace
pin above is now `defmt = "1.0"` (see this crate's `Cargo.toml`), so that specific conflict no longer
applies — the feature has simply never been re-evaluated since. Nothing depends on it either way:
`main.rs`'s `broker_task` renders the embassy-net address/config types by destructuring them into
primitive `{=u8}`/`{=u16:x}` defmt args, so it never needs a `defmt::Format` impl from these crates
at all.

## Device-hosted MQTT broker over W5500 (story S8 #359)

The device does **not** dial out to an external broker — it **hosts** one. `broker_task`
(`ns/src/main.rs`) accepts TCP connections on **port 1883** and serves unmodified stock MQTT 3.1.1
clients, fanning the Seplos telemetry it receives on the `MqttUnit` channel out to whichever of them
have subscribed. That is the standing device-hosted-broker doctrine (`lib-net`'s `link::mqtt_server`
module doc, Phase-2 Design Decision D1): the MCU is the server, the bench Pi/laptop is the client.

**Claims:** up to `MAX_CLIENTS` = 3 concurrent MQTT 3.1.1 clients, QoS 0, `#`/`+` topic-filter
wildcards, retained-topic replay to a late-joining subscriber, all driven by `lib-net`'s generic
`lib_net::embassy::pump::async_broker_pump` over `embassy-net` `TcpSocket`s — no protocol code lives
in this crate. **Non-claims:** plaintext 1883 only (no TLS, no `tls` feature here yet — `lib-net`'s
rustls `TlsStage` is not wired to this socket); no client authentication or ACLs; QoS 0 only (QoS
1/2, i.e. PUBACK/PUBREC/PUBREL/PUBCOMP, are explicitly out of scope in `mqtt_server`); the MAC is
still the fixed locally-administered placeholder from `boot_w5500.rs` and the `embassy-net`
`random_seed` is a hardcoded `42` (both TODO before more than one board shares a segment).

### The three W5500 tasks

`boot_w5500::spawn_net_and_broker` spawns exactly three tasks (all declared in `main.rs`), after
`boot_w5500::build_w5500` has assembled the stack:

1. **`w5500_wiznet_runner_task`** — the `embassy-net-wiznet` driver `Runner`: the SPI0 frame pump,
   gated on the W5500's `INTn` line. Never returns.
2. **`w5500_net_runner_task`** — `embassy-net`'s own IP-stack `Runner` (ARP/NDP, the DHCPv4 client,
   the `smoltcp` interface poll). Never returns.
3. **`broker_task`** — waits for DHCP, announces the address over RTT, then runs the broker pump
   forever.

If `build_w5500` returns an error (W5500 absent or mis-wired — the chip never responds on SPI0),
`main` logs `W5500 init failed` and **returns from `main` without spawning any of the three**. The
Seplos poll loop, the parser task and the watchdog are already running as independent tasks at that
point, so telemetry keeps flowing into the channel; the networking leg simply never comes up. The
DDS tasks below are spawned after the same bring-up and are lost with it.

### Broker pump

`broker_task` holds four `'static` resources handed to it by `spawn_net_and_broker`:

| Resource | Type | Sizing |
|---|---|---|
| `BROKER_CORE` | `BrokerCore<MAX_CLIENTS>` | 3 client slots; stores **no payload bytes** — only active/retained topic strings and per-client pending-publish topic lists |
| `LATEST_STORE` | `LatestStore<8, 1024>` | 8 topics x 1024 B ≈ 9.2 KiB, against the type's 32x8192 = 260 KiB default — this is the `LATEST_STORE` right-size the "Memory split" section above credits for 76.3% -> 20.2% static RAM |
| `W5500_RX_BUF` | `[u8; 512]` | per-read socket scratch, reused across all slots |
| `PAYLOAD_SCRATCH` | `[u8; MAX_PAYLOAD]` | drain-write scratch (`lib_mapping::sink::MAX_PAYLOAD`) |

The accept source is a closure, not a task: it reserves a buffer pair from `SOCKET_POOL`
(`SocketBufferPool`, `POOL_N_BUF` = `MAX_CLIENTS + 1` = 4 pairs), builds a `TcpSocket` with a 30 s
timeout and `accept(1883)`s on it; with the pool exhausted it backs off 50 ms and yields `None`. The
returned `PooledSocket` carries the reservation, and **field order is the drop contract** — the
socket drops before the reservation, so a freed pair is never still borrowed. This matters because
the pump races accept in a `select3` and drops the loser on every iteration; see
`ns/src/types/socket_buffer_pool.rs` for the full reclaim + cancellation-safety argument.

`async_broker_pump` then races three things forever: accept a new client, round-robin read the
connected slots (`feed_incoming` → drain), and receive an `MqttUnit` from the device channel
(`store.upsert(topic, payload)` → `publish_topic(topic, retain = true)` → drain **all** slots). The
`retain = true` is what makes a client that connects mid-stream get the current value of every topic
on SUBSCRIBE rather than waiting for the next poll cycle — payload bytes are rendered on demand out
of `LatestStore` at drain time, never replayed from a stored copy.

Topics are `{prefix}/{device_id}/{snake_case(tms_topic)}` (`lib_mapping::sink::topic_into`), which
with this crate's `run_config()` (`prefix = "tms"`, `DEVICE_ID = "METEOR-002"`) yields e.g.
`tms/METEOR-002/dc_measurement_update`.

### Socket budget (a fixed bug, do not re-break)

| Constant | Value | What it counts |
|---|---|---|
| `MAX_CLIENTS` | 3 | broker client slots |
| `POOL_N_BUF` | 4 | rx/tx buffer pairs = 3 serving + 1 accepting `TcpSocket` |
| `STACK_SOCKETS` | 7 | the whole smoltcp socket table: 3 serving + 1 accepting + 1 DHCP + 1 DNS + 1 spare |

`STACK_SOCKETS` is `MAX_CLIENTS + 4`, **not** `MAX_CLIENTS + 1`. An earlier version conflated the
app's socket count with the total socket table: once DHCP(1) + 2 clients + 1 accepting filled a
4-entry table, the third client's `accept()` failed silently — no panic, no log, the device just
capped at 2 live clients. The buffer pool is deliberately *smaller* than the socket table because
only real TCP sockets need an rx/tx buffer pair; DHCP/DNS entries do not. Full derivation in
`boot_w5500.rs`'s module doc.

## DDS/RTPS TMS telemetry over UDP/IPv6 (EPIC #328)

Separately from the broker, and on the same `embassy-net` stack, two always-compiled tasks
(`spdp::spdp_task` and `dds_pub::dds_pub_task`, both spawned from `main` with the `Stack` handle and
`W5500_MAC`) speak **DDS/RTPS over UDP/IPv6 multicast** — the MIL-STD-3071 transport
(`docs/compliance/tms/`), which mandates RTPS/UDP/IPv6 and is why `embassy-net` here enables
`proto-ipv6` + `multicast` + `slaac` on top of `dhcpv4`. Both tasks only ever `.await` a timer or a
non-blocking `send_to`, so neither can starve the broker or the poll loop on the shared executor.

**Claims:** a real DDS stack (Eclipse CycloneDDS, reader built from the idlc-compiled MIL-STD-3071
IDL) discovers this participant over SPDP, matches the `DcMeasurementUpdate` writer over SEDP, and
decodes the published sample — HIL-verified; the reproduction is `docs/testing/mcu-dds-spdp-hil.md`.
**Non-claims, stated plainly:**

- **The DATA sample is synthetic.** `dds_pub/impl_dds_publisher.rs::synthetic_reading()` is a fixed,
  plausible `CanonicalReading` (4 cells, 2 temps, a modest discharge current). Live Seplos telemetry
  reaches the **MQTT broker** leg today, not the DDS leg — wiring the real reading through is a
  later step.
- **QoS is a test simplification.** The SEDP announce advertises BEST_EFFORT / VOLATILE, not the
  production TMS Annex-D.5 profile (`lib_tms::generated_qos`, which is Reliable). Best-effort
  sidesteps the reliable-writer HEARTBEAT/ACKNACK state machine for the interop proof.
- **The vendor id is custom.** `0xFF01`, deliberately clear of the OMG-allocated ids (RTI `01 01`,
  eProsima `01 0f`, OpenDDS `01 03`, dust_dds `01 14`).
- **`ff02::` is link-local scope.** The announce never crosses a router, so any verifier must sit on
  the device's own L2 segment — a host that can merely *route* to the device will see nothing.

### Parameters (project ground truth — domain 0)

| Item | Value |
|---|---|
| DDS domain | `0` |
| Multicast group | `ff02::1234` (project-specific; deliberately **not** the RTPS-default `239.255.0.1`) |
| SPDP metatraffic multicast | port `7400` (`PB + DG*0 + d0`) |
| Metatraffic unicast | port `7410` |
| User-data default multicast | port `7401` — where the `DcMeasurementUpdate` DATA goes |
| User-data default unicast | port `7411` — the port `dds_pub_task` binds |
| SPDP encoding | PL_CDR_LE (`00 03 00 00`) |
| Sample encoding | XCDR2 / D_CDR2_LE (`00 09`), rendered by `lib_mapping::render_cdr::render_tms_cdr` |
| Topic / type | `"DcMeasurementUpdate"` / `"tms::dc::MeasurementUpdate"` |
| Participant lease | 100 s (announce cadence 3 s; publish cadence 2 s) |

**No RTPS library is linked** — dust_dds needs `alloc`. The submessage layout, parameter ids,
locator kind and byte order are hand-encoded, each constant carrying a `vendor/dust_dds/` file:line
cite plus the DDSI-RTPS 2.4 reference; the ~212 B announce is built into a reused `'static` buffer
via `ByteCursor`. See `ns/src/spdp/mod.rs` and `ns/src/dds_pub/mod.rs`.

### What each task emits

- **`spdp_task`** waits for link-up, joins `ff02::1234` (retrying until SLAAC has assigned the
  `fe80::` link-local), binds UDP `:7400`, then every 3 s sends one RTPS SPDP participant
  announcement to `[ff02::1234]:7400`. The advertised GuidPrefix is deterministic —
  `vendorId(2) ++ MAC(6) ++ 00 00 00 01` — and the unicast locator is the device's real SLAAC
  address, derived from the same MAC by the same EUI-64 math as `lib_net::embassy::link_local_v6`.
  `PID_DEFAULT_MULTICAST_LOCATOR` advertises `ff02::1234:7401` so a discovering reader knows to join
  the user-data group as well as the metatraffic one.
- **`dds_pub_task`** binds UDP `:7411` and every 2 s sends two datagrams: an SEDP
  `DiscoveredWriterData` **plus a HEARTBEAT** to `[ff02::1234]:7400`, and the `DcMeasurementUpdate`
  DATA sample to `[ff02::1234]:7401`. The HEARTBEAT is load-bearing — without it a peer's reliable
  builtin SEDP reader stays `out-of-sync` and never matches the user writer. Egress multicast needs
  no join, so this task never joins anything.

### Watching both legs over RTT

Flash and attach with `just mcu-flash-device` + `just mcu-rtt` (or the manual OpenOCD flow in
`just mcu-rp2350-rtt`). With `--features defmt` on a device that links and DHCPs, expect roughly:

```
net: hostname meteor-pico
w5500: init ok — waiting for link + DHCP…
dhcp: acquired 10.200.0.195/24
net: gateway 10.200.0.1
broker up: 10.200.0.195:1883
net: ipv6 fe80::ff:feaa:bbcc
mqtt: broker up :1883
spdp: joined ff02::1234
spdp: announce sent 212B
dds: sedp announced 240B seq 1 hb 1
dds: data sent 312B seq 1
mqtt: client connect
mqtt: publish
mqtt: client close
```

(The exact SEDP/DATA byte counts and the DHCP address are whatever your segment hands out; the first
announce of each kind is additionally hexdumped in full so the wire format can be validated offline
against the RTPS spec.) With `defmt` off — the shipping default — every one of these paths runs
identically and silently.

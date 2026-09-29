# 0012 — Delight features: board move, four new seams, and what they do and don't claim

**Status:** Locked (2026-07-25).

## Context

The contracted scope for this delivery is done. Four additions — beyond the original proposal —
were planned to delight the customer at handover, likely the final feature work before delivery:

1. **Out-of-band LoRa mesh that actually works** — Meshtastic over UART to a Heltec V4, driven from
   the MCU, plus fixing two bugs (verified field numbers, the falsified `MeshPacket.to` assumption
   — see the superseded ADR [0006](./0006-meshtastic-protobuf-approach.md)) that made the existing
   mesh path silently useless.
2. **Multi-rail power telemetry** — an INA3221 measuring the W5500 and Heltec rails.
3. **On-device UI** — a 128x32 OLED FeatherWing, four screens, three buttons.
4. **The board move that enables all of it** — Pico 2 → Adafruit RP2350 Feather (8 MB flash).

This record covers items 2-4 (the board move and the seams it enables) plus states, explicitly, what
this round of work does and does not claim about power measurement and BMS control. Item 1's own
framing/protocol decision is ADR [0006](./0006-meshtastic-protobuf-approach.md) (superseded
2026-07-25, same day as this record).

## Decision

### The board move: Pico 2 → Adafruit RP2350 Feather

Same silicon (RP2350), different board. The Feather was chosen because an OLED FeatherWing and an
INA3221 breakout both **stack natively** on its FeatherWing header / STEMMA QT connector, sharing
one I2C1 bus (SDA=GPIO2, SCL=GPIO3) with **zero extra GPIO** — the alternative (a bare Pico 2 plus
breakout wiring for both parts) would have consumed several more pins against an already tight
budget (16 of ~21-23 exposed GPIO once W5500 + Seplos + Heltec UART + I2C + buttons are all
accounted for). The move costs an 8 MB flash window relocation and a full pin remap (I2C1 lands on
GPIO2/3, which collides with the RS-485 DE pin from the Pico 2 map — the remap is forced, not
optional) but is otherwise a drop-in swap of the same MCU family; see the Feather pinout
ground-truth note (Basic Memory) for the verified 16-pin assignment table.

### Four seams added

- **`RailMonitor`** (`spec-app`) — `async fn read_all(&mut self, out: &mut [DeviceRailReading]) -> usize`,
  hung off `Platform` as a default-no-op sibling of the existing `emit_inverter_reading` hook via
  `Platform::emit_rail_draw` (additive — `bin-rpi-platform` compiles and behaves unchanged). Backed
  by an **owned** async INA3221 driver, not a crate (see below). **Diagnostics/UI only — see the
  correction recorded below**; this is not a TMS mapping.
- **The Meshtastic mesh sender** — `crates/lib-net::mesh`'s codec went `no_std`/no-alloc so **both
  targets ride the identical encoder**: the Pi's existing tokio `run_radio` loop and the MCU's new
  mesh sender (`bin-mcu-platform`) both call the same buffer-based `condense`/`to_radio_frame`
  functions. A divergent std copy of a codec just rewritten to be shared would have been strictly
  worse — one codec, two callers, per the umbrella plan's locked decision.
- **`DrawTarget`** (`embedded-graphics` 0.8.2) — the on-device UI's render seam. Four screens
  (`crates/bin/mcu-rp2350/ns/src/ui/`) render generically against `DrawTarget<Color = BinaryColor>`, so
  the SSD1306-backed OLED FeatherWing and a host `MockDisplay` are both valid targets with **no
  hardware, no SDL, and no allocator** required for CI.
- **The INA3221 driver** (`crates/lib-io/src/ina3221/`, feature `ina3221`) — an owned async driver
  against `embedded_hal_async::i2c::I2c`, ~6 registers per the TI INA3221 datasheet.

### Why the INA3221 driver is owned, not dependency-pulled

The `ina3221` crate is **0.4.5, last released 2023-02-20** — over three years stale — and depends
on **`embedded-hal ^1.0.0-alpha.9`**, a **pre-1.0 alpha** that cannot compose with
`embedded-hal 1.0` or embassy's current traits. This is the same trap the OLED research
independently flagged for the `sh1107` driver (`embedded-hal-async 0.1.0-alpha.1`, one release,
2022-09-03) — a pattern worth naming: in this ecosystem corner, a crate's *apparent* fit (it does
the right thing) is not evidence of *actual* fit (it composes with the HAL version this firmware
is built against) until the `embedded-hal` version pin is checked. Writing the driver is a small,
bounded surface (~6 registers) and buys three things a stale alpha-pinned dependency cannot: an
async impl that shares the display's I2C1 bus with no second bus and no extra GPIO;
host-testability against a mock I2C; and no stale pre-1.0 alpha dependency in a defense-adjacent
SBOM. Same reasoning as the hand-rolled Meshtastic encoder (ADR 0006) and the `embedded-gui`
rejection below — this project owns small, verifiable surfaces rather than depending on
unmaintained ones, even when "just use the crate" looks cheaper at first glance.

### Why `embedded-gui` was rejected

The name resolves to **two unrelated projects**, and neither is safe to depend on:

- The crates.io crate (`leftger/embedded-gui` 0.1.2, published 2026-06-06, most recently
  2026-07-19) is **~7 weeks old with 66 total downloads, 6 stars, 0 forks, 0 watchers, and a single
  author** — roughly 17K SLoC of widgets/charts/dialogs/spring-physics animation with essentially
  zero external review or field use.
- The GitHub project most web searches surface instead, `bugadani/embedded-gui`, is **effectively
  abandoned** (last push 2023-03-19, ~3.4 years stale at time of research) and its own README
  **self-describes as "(bad)"** and "experimental." It was never published to crates.io under this
  name.

For a defense-adjacent TMS device carrying SBOM + `cve-check` gates, either option is a bus-factor
and supply-chain risk far exceeding the benefit — especially against a requirement this modest: four
screens of static text/number readouts, no sliders, dropdowns, dialogs, or animation. A widget
toolkit is the wrong altitude for "draw six lines of text and switch between four of them."

**`slint` and `lvgl` were also rejected**, both for the same underlying reason: **both require a
global allocator**, which this `no_std`/no-alloc MCU target does not have and is not adding.
`slint` 1.17.1 additionally targets colour/framebuffer displays and its reference MCU port assumes
264 KB RAM for a task this device does in ~1 KiB; `lvgl` (via `lv_binding_rust`) is stale (last
release 2023-04-02) and its C FFI shim would directly contradict this codebase's "keep the FFI/C
shim DUMB" doctrine. The chosen path — plain `embedded-graphics` + a hand-rolled `enum Screen` +
render-trait — is the ecosystem bedrock every display driver already targets, and is fully
host-testable via `embedded_graphics::mock_display::MockDisplay` with zero hardware.

## Claims and non-claims — stated explicitly

**We do NOT measure total board draw.** The Feather's own 3.3 V plane is not instrumentable: the
INA3221's sense loop is an isolated terminal + shunt with no connection to the board's own rails,
and the Feather RP2350 documents no supported cut point for interposing a shunt in that plane (only
CHG and EN jumpers exist, and neither helps). Measuring total board power would require an
unsupported PCB trace cut. What we measure is two **external module rails** (W5500, Heltec), not
the sum of everything the board draws — do not describe these two channels as board power in any
downstream reporting.

**We CANNOT resolve microamp idle current.** The INA3221's resolution floor is ≈800 µA/count on its
0.05 Ω shunts (versus ≈100 µA on an INA219's 0.1 Ω shunt). This is fine for a W5500 at ~150 mA or a
Heltec TX burst, but it cannot answer the µA-scale idle-current question — that question stays
open regardless of this work landing.

**We do NOT command the BMS.** The Seplos link stays strictly read-only, per the standing Seplos
read-only doctrine (Basic Memory): the vendor documentation names write op-codes but never
documents a payload format for them, every mature integration observed is read-only, and the BMS is
the autonomous safety actor for cell balancing/protection — this project observes and reports, it
does not actuate. Nothing in this round of work changes that boundary; the INA3221/mesh/UI seams
are all read/observe/display paths, none of them write to the BMS or the inverter.

## Correction (2026-07-26): the INA3221 rails are NOT TMS mission data

This record's original landing mapped the INA3221 readings onto
`DcMeasurementUpdate.internal_measurement` — a real TMS field, but the wrong one to use here. That
was a mistake, corrected in place:

- **The Seplos BMS is the project's source of power information** — pack voltage, current, SOC.
  That is the mission telemetry, and it was already mapped correctly (`lib_mapping::transform.rs`)
  before any of the INA3221 work landed; that leg was untouched by this correction.
- **The INA3221 measures the adapter board's own rail draw** (W5500 + Heltec) — engineering
  self-telemetry about the hardware we built, not about the mission the device reports on. Mapping
  it onto `internal_measurement` put it on the SAME TMS topic a consumer reads for the pack, so a
  TMS consumer would see our own board's rails presented alongside battery-pack data — device
  housekeeping dressed as standards-compliant mission telemetry.
- **The fix:** the `lib_mapping::transform_power` module (the TMS mapper) is deleted outright —
  `lib-mapping` now has no knowledge of rail telemetry at all. The canonical types were renamed so
  the distinction is unmistakable going forward: `CanonicalPowerReading` → `DeviceRailReading`,
  `PowerRail` → `DeviceRail`, `spec_app::PowerMonitor` → `spec_app::RailMonitor`,
  `Platform::emit_power` → `Platform::emit_rail_draw`. The rail readings now publish as a small,
  non-TMS diagnostic JSON payload (`bin_mcu_platform::publish_rail_draw`, topic `diag/rails`) and
  render on the OLED UI's About/device screen — explicitly labeled as device draw, not on the
  Battery screen, so it can never again be mistaken for Seplos-sourced pack data (see
  `crates/bin/mcu-rp2350/ns/src/ui/types/ui_snapshot.rs`'s module doc for the UI-side detail).
- Every other claim/non-claim in this record (total-board-draw, resolution floor, BMS read-only
  posture) is unaffected — this correction only changes WHERE the rail readings go, not what the
  INA3221 measures or how.

## Consequences

- The four seams are additive to the existing `Platform`/`SerialTransport`/`Publisher` contracts
  (see [`architecture.md`](../architecture.md) §2 and the new §6 this record's landing added) —
  `bin-rpi-platform` compiles and behaves unchanged; nothing on the Pi path is touched by this
  decision.
- Hardware validation for the mesh path specifically has **not** happened yet (see ADR
  [0006](./0006-meshtastic-protobuf-approach.md)'s revisit trigger) — say "implemented, HIL
  validation pending," not "verified," for the mesh sender until a real Meshtastic radio run
  confirms delivery.
- The INA3221 and OLED work both trade a small amount of owned/hand-rolled surface for avoiding a
  stale or risky dependency — consistent with, and reinforcing, the same call already made for the
  Meshtastic encoder in ADR [0006](./0006-meshtastic-protobuf-approach.md).

### A lesson worth recording: untyped cross-tree references are where staleness hides

Every stale reference corrected in this delivery round — ADR 0006's own status banner (claiming a
stub that had long since become a full encoder), three source-code doc comments in
`crates/lib-net/src/mesh/` citing a file path (`impl-app/src/host/impl_host_tap.rs`) that predates
two crate reorganizations, and the matching path mentions in
`docs/reports/gap-analysis-meteor-parity.md` — share one root cause: **an untyped cross-tree
comment or doc reference that nothing compiles, links, or doc-tests.** A Rust `use` statement
breaks the build the moment its target moves; a doc comment or markdown link pointing at another
crate's file path has no such enforcement — only a human re-grep catches the drift, and by
construction that only happens when someone goes looking. None of these staleness instances were
caused by carelessness at write time — every one was accurate when written. They went stale later,
silently, because nothing in the toolchain would have told anyone. Treat this as a standing hazard
class, not a one-off cleanup: prose that names a path is a promise the compiler cannot check.

## Revisit trigger

- A real Meshtastic radio HIL run either confirms delivery (supersede ADR 0006's "implemented,
  pending" status to "verified") or surfaces a further framing bug.
- If total-board-draw measurement is later required, this record's non-claim needs an explicit
  revisit — it would require a hardware revision (a supported shunt-insertion point) that the
  current Feather does not offer.
- If BMS write access is ever pursued, that is governed by the separate Seplos
  write/command-capability research (Basic Memory) and would need its own ADR — this record's
  "read-only" non-claim does not itself change without that separate decision.

# Delight-Features MCU Bench — Wiring, Heltec Config, HIL Sign-off

How the "delight features" hardware (Meshtastic mesh over a Heltec V4, INA3221 multi-rail power
telemetry, and the OLED FeatherWing UI — see the umbrella delivery plan) lands on the existing
[HIL bench](#relationship-to-the-existing-two-pico-bench), and the checklist an operator runs on
`sentient-pi-bench` to sign the work off as HIL-verified — the plan's own "done bar", not merely
build-green.

**Board note:** the DEVICE node in this bench is now an **Adafruit Feather RP2350** (8 MB flash),
not the bare Pico 2 the original two-Pico topology doc describes — see
[Relationship to the existing two-Pico bench](#relationship-to-the-existing-two-pico-bench) for
exactly what that changes and what stays the same. All GPIO numbers below are the Feather's own
pin table, verified against four independent Adafruit/RP2350 primary sources (schematic, prettypins
silkscreen diagram, LEARN guide raw HTML, and the RP2350 datasheet's GPIO function table).

## Pin map (Feather RP2350, verified)

| Function | GPIO | Silk label |
|---|---|---|
| W5500 SCK | GPIO22 | SCK |
| W5500 MOSI | GPIO23 | MOSI (MO) |
| W5500 MISO | GPIO20 | MISO (MI) |
| W5500 CS | GPIO10 | D10 |
| W5500 INT | GPIO11 | D11 |
| W5500 RST | GPIO26 | A0 |
| Seplos UART0 TX | GPIO0 | TX |
| Seplos UART0 RX | GPIO1 | RX |
| Seplos UART0 DE (RS-485 direction) | GPIO29 | A3 |
| Heltec UART1 TX | GPIO24 | D24 |
| Heltec UART1 RX | GPIO25 | D25 |
| I2C1 SDA (shared: INA3221 + OLED) | GPIO2 | SDA |
| I2C1 SCL (shared: INA3221 + OLED) | GPIO3 | SCL |
| Button A | GPIO9 | D9 |
| Button B | GPIO6 | D6 (only button with an onboard 100 kΩ pull-up) |
| Button C | GPIO5 | D5 |

No GPIO is used twice; 16 of ~21-23 exposed pins are spoken for. Buttons A and C need firmware
`Pull::Up` explicitly enabled — only B has the onboard pull-up (added originally for an ESP8266
reference design) — the single most likely bring-up bug on this board.

## Seplos serial leg — Feather ↔ Pico 2W simulator

**The Adafruit 5994 USB multi-protocol cable is NOT used on this leg, and should not be cut open.**
This reverses what an earlier version of the plan assumed, so it's worth stating why plainly:

- The 5994 is a **USB cable** (FT232RNL + UM213 RS-232 shifter + MAX485 RS-485 transceiver, all
  permanently paralleled on one FTDI UART, per the manufacturer's assembly drawing PCM-KW-469).
  Protocol selection is **which screw terminal you physically land a wire on** — there is no
  switch, jumper, or software command, so cutting it open buys no GPIO-selectable multi-protocol
  capability at all.
- Cutting it open to drive it from the Feather would additionally mean: desoldering the FTDI's TXD
  pin to avoid output-vs-output contention with the RP2350 GPIO, re-injecting 5 V to power the
  UM213/MAX485 (both 5 V parts), **level-shifting their 5 V receive lines into the RP2350** (whose
  5 V-tolerance language was removed from the datasheet — Raspberry Pi's own engineers now recommend
  treating the pins as 3.3 V-only and using external level shifters), and soldering directly to the
  MAX485's DE/RE pins to get direction control (stock, direction is FTDI CBUS-auto, which dies with
  the chip). None of that is reversible, and the housing's construction (glued/welded/potted) isn't
  published anywhere.
- The Feather↔Pico-2W leg is MCU-to-MCU with **no USB host present on either end** — the 5994's
  USB-side value proposition doesn't apply here at all. It already works as a bare 3.3 V TTL wire,
  exactly like the existing `hil/pico-seplos-emu` bench (see `hil/README.md`'s Pi↔Pico wiring),
  so there is nothing to bypass a USB bus *for*.
- The cable's one legitimate fit, if it's ever needed, is unmodified, as a plain USB dongle on the
  **Pi side** talking to a **real** Seplos pack's RS-485 daisy-chain port (`/dev/ttyUSB*`) — a
  different leg entirely from this bench.

So the actual, correct wiring for this leg is a direct 3.3 V TTL crossed connection, matching the
existing `hil/pico-seplos-emu` pattern:

| Feather (Seplos UART0) | Pico 2W simulator | Note |
|---|---|---|
| GPIO0 (TX) | GP1 (UART0 RX) | crossed |
| GPIO1 (RX) | GP0 (UART0 TX) | crossed |
| GND | GND | common ground, required |
| GPIO29 (DE) | — (unused) | no RS-485 transceiver on this leg; park or leave unasserted |

**3.3 V only** — do not connect any 5 V rail between the two boards. Baud is **19200 8N1**
(`crates/bin/mcu-rp2350/ns/src/boot_seplos.rs:74`, matching `hil/pico-seplos-emu`'s own config and
`hil/sentient-bench.toml`). The Pico 2W simulator only answers **DIP address 0**.

If real-pack RS-485 benching is ever needed on this board, the defensible path is a **discrete 3.3 V
transceiver breakout** (MAX3485/SP3485/THVD1450-class) with DE+RE tied to GPIO29 — that's already a
blessed mode (`GpioDe`) in ADR 0005 (`docs/decisions/0005-rs485-direction-mechanism.md`), no ADR
change needed. `hil/README.md`'s Phase 2 section already anticipates exactly this kind of leg.

## Heltec V4 Serial Module configuration

The Heltec V4 exposes the Meshtastic Client API two ways, and they are **not interchangeable**:
the USB-C console speaks it natively at a fixed 115200 baud; the **Serial Module** (what the MCU
actually drives, over UART1) must be explicitly configured. Set on the Heltec via its own config
channel (BLE/USB, one time, before it's driven headless from the Feather):

```
serial.enabled = true
serial.mode    = PROTO      # exposes the same Client API the phone app speaks
serial.rxd     = <GPIO>     # see "open firmware trap" below
serial.txd     = <GPIO>     # see "open firmware trap" below
serial.baud    = 38400       # default; must match crates/bin/mcu-rp2350/ns/src/boot_mesh.rs's UART1 config
```

**Open firmware trap — meshtastic/firmware issue #7749 (OPEN as of fw 2.7.6).** The Heltec V3/V4's
USB-C console UART (pins 43/44) **cannot** be repurposed for the Serial Module — attempts to point
it there via `override_console_serial_port` don't persist, and that option is documented as valid
only for NMEA/CalTopo GPS modes, not the Serial Module. **Two spare GPIOs must be dedicated.**

Candidates within the config's valid ranges (`serial.rxd` accepts GPIO 1-39, `serial.txd` accepts
GPIO 1-33) on the **OLED** `env:heltec-v4` build: **6, 15, 16, 33**. **All four of those are taken
on the TFT build** (LGFX CS=15/DC=16/MOSI=33, buzzer=6) — **confirm against your board's silkscreen
and the exact `env:` you flashed before wiring**, don't assume the OLED-build free set applies.

Firmware floor: **≥ 2.7.20** for V4.3 hardware, **≥ 2.7.25** for V4-R8. The wired-up two GPIOs land
on the Feather's UART1 pins (GPIO24 TX / GPIO25 RX) — crossed, same 3.3 V-only rule as the Seplos
leg. See also issue #8543 ("Flasher fails to connect to Heltec V4").

## INA3221 rail insertion points

One **Adafruit INA3221** (PID 6062, STEMMA QT) rides the Feather's shared I2C1 bus (GPIO2/GPIO3 —
same bus the OLED wing uses), default address **0x40** (0x41 via solder-jumper alternate; no
collision with the OLED's 0x3C).

- **Channel 1 — W5500 rail.** Break the W5500 breakout's 3.3 V feed and insert it through the
  channel's IN+/IN- shunt terminals in series. Exact wire depends on which pin on your specific
  W5500 breakout carries its power feed — confirm against its silkscreen before cutting.
- **Channel 2 — Heltec rail.** Same approach on whatever supplies the Heltec V4's power input
  (its own USB-C/battery input, or a shared feed from the bench supply if you're powering it off
  the Feather) — interpose the shunt in series.
- **Channel 3 — spare.** Left open for a future rail.

**The Feather's own 3.3 V plane is deliberately NOT instrumented, and this is not a bench oversight
— there is no supported way to do it.** The INA3221's sense channels are isolated terminal + shunt
pairs with zero electrical connection to whatever they're not explicitly wired across; the Feather
RP2350 documents no cut point on its own 3V/BAT/USB nets for interposing a shunt (only CHG and EN
jumpers exist, and neither helps here). Reaching the Feather's own rail would require an unsupported
PCB trace cut. So what this bench measures is **two external module rails, not total board draw** —
state this plainly in any report, don't describe the sum as board power.

Resolution floor is ≈800 µA/count on the INA3221's 0.05 Ω shunts — fine for a W5500 at ~150 mA and
for Heltec TX bursts, but it cannot answer a µA-scale idle-current question.

## OLED FeatherWing UI

**Adafruit 128x32 OLED FeatherWing (PID 2900, SSD1306, I2C 0x3C)** stacks directly on the Feather's
header — same I2C1 bus as the INA3221, no extra wiring. Buttons **A = GPIO9, B = GPIO6, C = GPIO5**
(the FeatherWing's own tactile buttons, active-low to ground). Four screens (Network, Meshtastic,
Battery, About) cycle via A/C with B as select — see `crates/bin/mcu-rp2350/ns/src/ui/` for the render
code and `docs/decisions/` for the crate choice (`embedded-graphics` + `ssd1306`, no widget
toolkit).

**As of this doc, the hardware-wiring wave for this leg (`boot_ui.rs`, `boot_i2c.rs`, the shared
I2C1 bus construction, and the actual embassy task spawn) had not yet landed** — the `ui` module and
its `Screen`/`ScreenView` render layer exist and are host-golden-tested (`MockDisplay`), and are
declared behind `ns`'s `ui` Cargo feature, but nothing on the MCU side calls into them against real
hardware yet. Confirm `crates/bin/mcu-rp2350/ns/src/boot_ui.rs` exists and is wired into `main.rs`
before running the display-dependent sign-off steps below — if it doesn't exist yet, those steps
are not yet executable and should be treated as blocked, not failed.

## Relationship to the existing two-Pico bench

The base topology (see the "HIL Bench — Two-Pico RP2350 Topology" reference) is unchanged in
spirit: a bench Pi (`sentient-pi-bench`, now a Pi 5) is the SWD + RTT access rail for two MCU
targets, reached over `ssh john@sentient-pi-bench`. What changed:

- **DEVICE moved from a bare Pico 2 to the Feather RP2350.** The Feather's SWD debug port is a
  dedicated 3-pin JST-SH connector (SWCLK/GND/SWDIO), not exposed test pads like the bare Pico —
  **physically re-land the bench's DEVICE probe wiring onto that connector** before assuming the
  existing `device_swclk`/`device_swdio` (`justfile` vars, GPIO11/GPIO10 on the bench Pi's own
  header) still reach the right pins; those vars name the **bench Pi's** GPIOs and don't need to
  change, but the physical continuity from bench-Pi-GPIO to Feather-JST-SH is a fresh cable run —
  verify liveness with `just mcu-swd-probe` before trusting it.
- **SIMULATOR is unchanged** — still the Pico 2W running `hil/pico-seplos-emu`, SWD GPIO21/20.
- The bench's `just` recipes (`mcu-hil-setup`, `mcu-flash-device`, `mcu-flash-sim`, `mcu-rtt`,
  `mcu-swd-probe`) are unchanged and already point at the correct `crates/bin/mcu-rp2350/...` ELF paths
  — see the `justfile`'s "MCU HIL bench" group. `mcu-flash-device` builds `ns --features defmt`
  only; add whichever of `mesh`/`ui` this bench run needs, e.g.:
  ```sh
  cd crates/bin/mcu-rp2350 && cargo build --release -p ns --features "defmt,mesh"
  ```
  then flash the resulting ELF the same way `mcu-flash-device` does (or extend that recipe's
  feature list locally for the duration of the sign-off run).

## Known bench-capability gap: no live fault injection yet

The Pico 2W Seplos simulator (`hil/pico-seplos-emu`) is **Phase 1 scope only** — telemetry (`0x42`)
at DIP 0, no fault frames. `hil/sentient-bench.toml` says so outright
(`fault_every = 0  # the phase-1 responder doesn't implement 0x44 — never poll it`), and
`hil/README.md`'s own Phase 2 section (a second responder node, an RS-232/RS-485 electrical leg)
does not add fault injection either. **There is currently no way to make the physical simulator
hardware emit a live `0x44` fault frame on the wire.** This is a pre-existing gap, not something
introduced or closeable from this WP's file list (`hil/pico-seplos-emu-core` is out of scope here);
flagging it as a follow-up for whoever owns that crate next.

Until that lands, the fault-forwarding sign-off step below uses the best available substitute:
temporarily standing in a host machine as the "pack" on the Feather's Seplos UART (in place of the
Pico 2W simulator), replying to the Feather's poll with a hand-crafted, checksummed `0x44` response
built from the empirical byte/bit map in `crates/lib-parse/src/seplos/fault.rs` (bit 0 =
`high_voltage_protection`, matching the mesh codec's fault bitmask). This proves the same code path
(`condense` → mesh frame) that a hardware-native fault would exercise; it does not prove the Pico
2W simulator itself can do it, because today it can't.

## HIL sign-off checklist

Run on `sentient-pi-bench` (`ssh john@sentient-pi-bench`), following the existing flash +
RTT-over-SWD pattern (`just mcu-hil-setup` → `just mcu-flash-device` → `just mcu-rtt`). Numbered so
each step's pass/fail is unambiguous.

1. **Bench reachable.** `just mcu-hil-setup` — confirms openocd is present on the bench and the SSH
   path is up.
2. **Build + flash the DEVICE** with the features this run needs (at minimum `defmt`; add `mesh`
   for the mesh steps below):
   ```sh
   cd crates/bin/mcu-rp2350 && cargo build --release -p ns --features "defmt,mesh"
   ```
   then flash it (mirror `mcu-flash-device`'s scp + openocd program/verify/reset sequence, or extend
   that recipe's feature list for this run).
3. **SWD liveness on both targets.** `just mcu-swd-probe` (DEVICE, default pins) and
   `just mcu-swd-probe 21 20` (SIMULATOR) — expect `DPIDR 0x4c013477` / "Examination succeed" on
   both, not "Error connecting".
4. **Flash the SIMULATOR.** `just mcu-flash-sim`.
5. **RTT sanity.** `just mcu-rtt` — expect the DEVICE's normal boot sequence (W5500 link up, DHCP
   acquired) with no panics.
6. **Seplos leg live.** With both flashed and wired per the [pin table](#seplos-serial-leg--feather--pico-2w-simulator)
   above, confirm over RTT that the DEVICE is decoding telemetry from the Pico 2W simulator each
   poll cycle (no `telemetry_err` growth, per the existing bench convention in `hil/README.md`).
7. **Mesh frame reaches a second radio — this is the datum that proves #386 is fixed.** With the
   Heltec V4 configured per [above](#heltec-v4-serial-module-configuration) and wired to the
   DEVICE's UART1, power up a **second** Meshtastic-capable radio on the same channel (another
   Heltec, or any Meshtastic device) and watch its Client API stream (official `meshtastic` Python
   CLI over USB, or the phone app's message/node log) for a packet **originating from the DEVICE's
   node**. Confirm the packet is actually received and decoded — not just transmitted — since
   nothing else in this bench observes whether `MeshPacket.to` is correctly `0xFFFFFFFF`
   (broadcast) rather than the old, silently-undelivered `to: 0`. A `PRIVATE_APP` (portnum 256)
   payload arriving and JSON-decoding to plausible telemetry fields is the pass condition.
8. **Fault reaches the mesh — this is the datum that proves the fault-forwarding gap is closed.**
   Using the [substitute method above](#known-bench-capability-gap-no-live-fault-injection-yet)
   (or, once it exists, a real `hil/pico-seplos-emu-core` fault-injection feature), inject a `0x44`
   fault response with at least `high_voltage_protection` set. Confirm the same second radio from
   step 7 receives a mesh frame whose condensed JSON payload carries the `"f"` fault bitmask key
   with bit 0 set, within one poll cycle of the injected fault — proving the fault rode the
   out-of-band link, not just the normal telemetry path.
9. **Display renders all four screens.** *(Blocked until `boot_ui.rs`/`boot_i2c.rs` land — see
   [OLED FeatherWing UI](#oled-featherwing-ui) above.)* Power-cycle or reset the DEVICE and confirm
   the OLED shows the Network screen first; press A/C and confirm all four screens (Network,
   Meshtastic, Battery, About) are reachable in both directions, with no visibly clipped/garbled
   text and no crash/hang on the transition.
10. **Buttons debounce cleanly.** Rapidly press each of A/B/C several times; confirm no double-fires
    or missed presses (particularly A and C, which rely on internal — not onboard — pull-ups).
11. **INA3221 reports plausible current.** *(Same blocker as step 9.)* Confirm the Battery screen
    (or an RTT log line) shows a W5500 rail current in the tens-to-~150 mA range and a Heltec rail
    current consistent with LoRa idle draw.
12. **Heltec rail visibly spikes on TX.** Trigger a mesh transmit (e.g. repeat step 7) while
    watching the Heltec rail's current reading; confirm a clear, transient spike above its idle
    baseline coincident with the TX burst — this is the cross-check that the INA3221 is actually
    wired to the Heltec's power feed and not floating or mis-routed to the W5500 channel.

Steps 7 and 8 are the two safety-relevant proofs called out in the delivery plan — nothing else in
this bench (or in CI, which cannot exercise real RF at all) observes whether a mesh frame is
actually delivered and decoded by a peer, so these two steps are the whole reason the "done bar" is
HIL-verified, not build-green.

## What this document does not cover

- Gate-0 secure boot / TrustZone S↔NS verification (`mcu-rp2350-uf2`'s scope, separate from this
  delight-features bench work).
- CI-automatable coverage — none of the above runs anywhere but the physical bench; there is no
  RP2350 emulator (Renode/QEMU have no model for this chip), so this whole document is,
  deliberately, a manual procedure.

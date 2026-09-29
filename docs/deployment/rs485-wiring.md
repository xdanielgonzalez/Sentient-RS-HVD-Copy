<p class="eyebrow">Deployment · RS-485 Wiring</p>

# RS-485 wiring & transceiver prerequisites

**RS-485 is a physical layer; UART is a framing layer. They are not alternatives — a working Seplos
link needs both.** RS-485 defines how bits travel on the wire: a differential pair (A/B), ground-
referenced but common-mode tolerant, multi-drop, half-duplex. UART defines how bits are grouped
into characters: start bit, 8 data bits, no parity, 1 stop bit, at 19200 baud. A UART peripheral
emits those characters as **single-ended 3.3 V TTL** on one pin. A Seplos pack listens for a
**differential voltage between A and B**. Nothing in the MCU or the Pi converts one to the other.

That conversion is the job of an **RS-485 transceiver**, and one must physically exist on the board
between the UART pins and the pack. This is the failure this chapter exists to prevent: wire an MCU
UART straight to a Seplos and you get silence — no error, no fault, no log line. The firmware
happily drives its driver-enable pin into open air, transmits into a pin the pack cannot hear, and
reports read timeouts forever. Every layer above the wire is behaving correctly, which is exactly
what makes it expensive to diagnose.

## What the software already assumes

Both shipping targets are written as if a transceiver is present. The presence of a driver-enable
(DE) line in the code is itself the proof: DE only means anything to a transceiver, so a firmware
that toggles it has already assumed one exists.

### MCU leg (RP2350)

`crates/bin/mcu-rp2350/ns/src/boot_seplos.rs` brings up UART0 at **19200 8N1** and takes a DE pin
alongside the TX/RX pins:

| Signal | Pin | Source |
|---|---|---|
| Seplos UART0 TX | GP0 | `boot_seplos.rs:69` (`tx_pin: Peri<'static, PIN_0>`) |
| Seplos UART0 RX | GP1 | `boot_seplos.rs:70` (`rx_pin: Peri<'static, PIN_1>`) |
| RS-485 driver enable (DE) | **GP29** | `boot_seplos.rs:71` (`de_pin: Peri<'static, PIN_29>`) |
| Baud / framing | 19200 8N1 | `boot_seplos.rs:74`–`77` |

The pin is claimed as a plain output, idle low (receive), at `boot_seplos.rs:93`:

```rust
let dir_enable = Output::new(de_pin, Level::Low);
```

GP29 is deliberate: on the Adafruit Feather RP2350 (silk label `A3`) it is free, whereas GP2 —
which the earlier bare-Pico 2 layout used for DE — is I2C1's fixed SDA pin on this board. See
`boot_seplos.rs:50`–`55` for that rationale and
[the delight-features bench doc](../testing/mcu-delight-features-hil.md) for the full verified pin
table.

There is no transceiver on the current bench wiring — see the honesty section below.

### Pi leg (bin-rpi on Yocto Linux)

The Pi does not hardcode a direction mechanism. ADR 0005
([`docs/decisions/0005-rs485-direction-mechanism.md`](../decisions/0005-rs485-direction-mechanism.md))
ships a three-mode config seam, `lib_io::host::DirectionMode`, selected per port
(`0005-rs485-direction-mechanism.md:36`–`45`):

- **`RtsSoftware`** — the default. The driver flips RTS around the write, with an explicit drain
  (`flush` + `tcdrain`) before dropping DE, plus settle delays (5 ms pre-write, 10 ms post-write).
  Works on any UART; relies on software timing being good enough.
- **`KernelRs485`** — `TIOCSRS485`. The kernel driver hardware-times DE assertion; `set_direction`
  becomes a no-op afterwards. The most robust option, where the UART driver supports the ioctl.
- **`GpioDe`** — a dedicated GPIO line driven via `gpiocdev`, for HATs that do not wire DE to RTS at
  all.

All three modes assume a transceiver whose DE/RE input is reachable — over RTS, over the kernel's
hardware timing, or over a GPIO line. None of them creates a differential pair.

**Config note, stated precisely because it is easy to misread.** The shipping sample
`distro/deploy/config.example.toml` documents the GPIO mode but does **not** enable it. Lines 49–59
of that file are entirely commented out and show `direction = "rts"` as the default; `gpio_chip =
"gpiochip4"` and `gpio_line = 22` appear there as commented placeholders recording the legacy
prototype's Pi 5 HAT wiring (the same `gpiochip4` pin 22 ADR 0005 cites at line 10), not as an
active setting. A deployment that actually needs GPIO DE must uncomment `direction = "gpio"` plus
both `gpio_chip` and `gpio_line` — and see the field-verification debt below before doing so in
production.

## Choosing a transceiver

The customer is designing their own board, so what follows is a **part class and its constraints**,
not a bill of materials mandate. The requirements are fixed; the specific part is not.

Requirements for this link: **3.3 V logic supply** (the RP2350's pins are 3.3 V-only — Raspberry
Pi removed the 5 V-tolerance language from the datasheet and now recommends external level
shifting), **half-duplex** (RS-485 two-wire A/B), **an exposed DE pin** (the firmware drives
direction itself), and a data rate that only needs to reach 19200 baud.

### Non-isolated tier

| Part | Supply | Notes |
|---|---|---|
| TI **THVD1450DR** | 3–5.5 V | Wide supply range, so the same part works on a 3.3 V or 5 V board. |
| TI **SN65HVD72DR** | 3.3 V | 250 kbps slew-rate-limited. The slew limiting is a *feature* here — a 20 Mbps part on a 19200-baud bus radiates and reflects for no benefit. Generally the better fit for a slow BMS link. |

**Trap — SN65HVD3082E is a 5 V part.** It sits in the same family, appears in the same search
results, and is often cheaper, which is exactly why it gets picked by mistake. It is **not suitable
for a 3.3 V-only design**. Check the supply-voltage row of the datasheet, not the family name.

### Isolated tier

Galvanic isolation is common on industrial BMS links, where the pack ground and the logic ground
can differ by more than the transceiver's common-mode range and a ground loop can destroy parts on
both ends. Two caveats before choosing one:

- TI **ISO3082DWR** is **NRND** (not recommended for new designs). Do not start a new layout on it.
- TI **ISO1412DWR** is Active, but it is **full-duplex and not pin-compatible** with ISO3082. It is
  a redesign, not a drop-in substitution.
- Isolation roughly **doubles to triples** this line item versus the non-isolated parts above.

**UNVERIFIED:** whether these isolated families' logic-side rail (VCC1) will operate on a 3.3 V-only
board. They are documented as 5 V parts and the VCC1 *minimum* was not confirmed against the
datasheets during this write-up. If VCC1 turns out to require 5 V, an isolated design forces a 5 V
rail on the bus side plus level shifting — a real cost and layout consequence. **Confirm VCC1's
minimum against the datasheet before committing to an isolated part.**

## Bus wiring

- **A/B polarity.** Match A to A and B to B end to end. Reversed polarity is the classic symptom:
  the link is electrically fine and every exchange times out. Swapping A/B is the first thing to
  try on a bus that only ever produces timeouts.
- **Common ground.** RS-485 is differential but not ground-free; the transceivers still need their
  grounds within common-mode range of each other. Run a ground wire (or use an isolated part).
- **Termination: 120 Ω across A/B at both physical ends of the bus, and only there.** Not one per
  node. A multi-drop bus with a terminator on every node loads the driver down and the waveform
  collapses. A short bench run at 19200 baud often needs no termination at all.
- **Fail-safe biasing.** With no node driving, an unbiased pair floats and the receiver output is
  indeterminate — noise decodes as start bits and framing errors. Pull-up on A and pull-down on B
  (or choose a transceiver with integrated fail-safe receivers) so the idle bus reads as a mark.
- **ESD protection.** The bus leaves the board, so treat A/B as exposed. Bourns **CDSOT23-SM712** is
  purpose-built for RS-485: an asymmetric 12 V/7 V clamp matching RS-485's asymmetric −7 V to +12 V
  common-mode range, so it does not clamp legal bus levels.

## The bench HIL sign-off does NOT cover the RS-485 physical layer

This section is not a caveat to be smoothed over in a later edit. It is the current, accurate state
of verification.

**The MCU differential leg is unproven on real hardware.** The bench topology is DEVICE (Adafruit
Feather RP2350) to SIMULATOR (Pico 2W running `hil/pico-seplos-emu`), and both ends are **3.3 V TTL,
single-ended, with no transceiver anywhere in the loop**. The bench pin table states this outright:
GPIO29 (DE) is listed as unused, "no RS-485 transceiver on this leg; park or leave unasserted"
(`docs/testing/mcu-delight-features-hil.md:75`).

So the telemetry captured on that bench — the HIL sign-off for the W5500/broker work recorded 43
PUBLISH messages in 45 s at a steady ~2 s cadence with live drift (amperage 0.85 A to 5.56 A, pack
voltage ≈ 53.26 V) — **travelled over TTL, not RS-485**. That run proves the UART framing, the
Seplos protocol decode, the mapping, and the publish path. It proves **nothing** about differential
signalling, DE timing against a real transceiver, bus termination, biasing, or common-mode
tolerance. Those are exactly the things that break first against a real pack.

*(A note on figures, since a different triple circulates: 54.51 V / 1.44 A / 78.49 are the
`lib-mapping` doc-test and acceptance-suite sample values — and 78.49 there is **watts**, not
percent SOC; that fixture's SOC is 77.5 %. The bench simulator's own canned fixture is 53.29 V,
−1.64 A, 77.5 % SOC, derived in
`hil/pico-seplos-emu-core/src/seplos_responder/impl_battery_state.rs`. None of these are a
transceiver measurement.)*

**The Pi side carries the same debt, already on record.** ADR 0005's Consequences section states it
plainly: the drain/settle *sequencing* is pty-provable and covered by CI
(`lib-io/tests/direction_mode.rs`), but "a pty implements none of the modem-control ioctls
`write_request_to_send`/`TIOCSRS485`/GPIO toggling actually need — **none of the three modes has a
real 'it correctly asserts DE on real silicon' proof yet**", and "field verification is explicitly
still owed, not closed by this decision"
(`docs/decisions/0005-rs485-direction-mechanism.md:56`–`64`). `lib-io/src/host/gpio_de.rs:14`–`17`
repeats it for the GPIO path specifically: no `/dev/gpiochipN` exists in CI, so neither a real
request/set-value round-trip nor the adequacy of the default settle delays has been observed.

**This chapter extends that recorded Pi-side debt to the MCU side.** Until someone runs a Feather
RP2350 through a discrete 3.3 V transceiver into a real Seplos pack and captures telemetry, the
RS-485 physical layer is designed-for and coded-for, but not demonstrated, on either target.

The intended path for closing the MCU half is already blessed and needs no ADR change: a discrete
3.3 V transceiver breakout (MAX3485/SP3485/THVD1450-class) with DE and RE tied together to GPIO29 —
which is ADR 0005's `GpioDe` mode on the Pi and the plain `Output` at `boot_seplos.rs:93` on the MCU
(`docs/testing/mcu-delight-features-hil.md:82`).

## Bring-up checklist for a new board

1. Confirm a transceiver is populated and its logic supply matches the MCU's 3.3 V rail. Read the
   supply row of the datasheet for the exact part fitted, not the family page.
2. Confirm DE (and RE, if separate) reaches GP29 on the MCU, or the configured `gpio_line` / RTS pin
   on the Pi. A transceiver with DE strapped permanently low can only ever receive.
3. Confirm a common ground between the transceiver and the pack, or that the part is isolated.
4. Confirm 120 Ω sits at both bus ends and nowhere else.
5. Confirm fail-safe bias, or a receiver with it integrated.
6. Only then bring up the poll loop. If it produces nothing but timeouts, swap A and B before
   changing any software.

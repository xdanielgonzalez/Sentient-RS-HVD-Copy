# HIL Bench — Pico Seplos BMS Pack Emulator (Phase 1)

A hardware-in-the-loop bench: a Raspberry Pi Pico runs firmware that speaks the Seplos BMS
protocol as a slave (pack emulator) over raw 3.3V TTL UART, wired directly to a Raspberry Pi 5's
GPIO UART. The Pi 5 runs the real, unmodified `bin-rpi` binary, polling the Pico exactly as it
would poll a real pack. This closes the gap between the pty-simulated pipeline (`test-e2e`'s
`SeplosSim`, see the `run-bin-rpi` skill) and a real device on the wire, without needing an actual
Seplos pack.

Two crates:
- `hil/pico-seplos-emu-core` — pure, `no_std` Seplos responder protocol logic (host-testable, zero
  hardware deps). A real sentient-rs workspace member.
- `hil/pico-seplos-emu` — the embassy-rp firmware binary (UART0 hardware binding). Excluded from
  the workspace (own toolchain/target), mirrors `bin/zephyr`.

Phase 1 scope: telemetry (`0x42`) only, DIP address 0, always-success (RTN=0x00), canned
`BatteryState::sample_14_cell()` data. No fault injection, no multi-DIP, no scripted scenarios —
see [Phase 2](#phase-2-forward-compat-designed-not-built) below for what's deliberately deferred.

## Wiring

Raw 3.3V TTL UART, crossed TX/RX, common ground. No RS-232/RS-485 electrical layer — this is a
direct GPIO-to-GPIO bench leg, only needed against a real pack (see Phase 2).

| Pi 5 (40-pin header)        | Pico                | Note |
|------------------------------|----------------------|------|
| GPIO14 / pin 8 (UART0 TXD)   | GP1 (UART0 RX)       | crossed |
| GPIO15 / pin 10 (UART0 RXD)  | GP0 (UART0 TX)       | crossed |
| GND (any GND pin, e.g. pin 6)| GND                  | common ground, required |

**3.3V only** — neither board tolerates 5V on these lines. Do not connect Pico VBUS/5V to the Pi.

The 40-pin header pinout (GPIO14/pin 8, GPIO15/pin 10, GND) is identical across every 40-pin Pi, so
the **wiring above is the same on a Pi Zero 2 W** — only the Pi-side *device node* differs by board.
The `/dev/ttyAMA0` above is the Pi 5 case; verify yours per the
[Smoke test procedure](#smoke-test-procedure) (the repo's `docs/deployment.md` §2.1 table is flagged
provisional).

### Pi Zero 2 W bench (VERIFIED 2026-07-22)

On the Pi Zero 2 W (BCM2837) the header UART is **`/dev/serial0` → `/dev/ttyS0` (the mini-UART)** —
**there is no `/dev/ttyAMA0`** here (the PL011 enumerates as `ttyAMA1` and is held by Bluetooth).
`enable_uart=1` (already set in `/boot/firmware/config.txt`) locks the VPU core clock, so the
mini-UART is stable at the bench's 19200 baud — no `core_freq` pinning needed.

**Three one-time host steps are REQUIRED** (verified end-to-end on the bench 2026-07-22):

1. **Route the UART onto GPIO14/15 with `dtoverlay=disable-bt`.** On this Debian Trixie image
   `enable_uart=1` did NOT apply the pinmux — `raspi-gpio get 14,15` reported `MUX UNCLAIMED`, so
   bytes written to the serial node never reached the header pins (a probe just read a floating
   line). Add `dtoverlay=disable-bt` to `/boot/firmware/config.txt` and reboot: this disables
   Bluetooth and moves the full **PL011** onto GPIO14/15 (auto-pinmuxed), so **`/dev/serial0` now
   points at `/dev/ttyAMA0`**. (This is NOT merely "hardening" — without it there is no UART on the
   header here. The mini-UART alone was never muxed.)
2. **Free the port from the serial console** (a `serial-getty` + kernel console hold it out of the
   box, so `bin-rpi` cannot open it):
   ```sh
   sudo systemctl disable --now serial-getty@ttyAMA0.service serial-getty@ttyS0.service
   # remove the console=serial0,115200 token (keep console=tty1) from the kernel cmdline:
   sudo sed -i 's/console=serial0,[0-9]* //' /boot/firmware/cmdline.txt
   sudo reboot
   ```
3. **Point the bench config at it** — `hil/sentient-bench.toml`'s `[serial].port = "/dev/serial0"`
   (→ `ttyAMA0` after step 1), or override without editing: `SENTIENT_SERIAL__PORT=/dev/serial0`.

Verify the UART leg without the full stack: with the Pico flashed and wired, send the DIP-0
telemetry request and expect a `0x42` response frame back —
`printf '~20004642F0010FD67\r'` at 19200 8N1 on `/dev/serial0` returns `~20004600A08E…\r`.

## Bench topology

```
 Pi 5 ─────────────────── bin-rpi (unmodified) ──── polls Seplos over /dev/tty* ────┐
  │  GPIO14 (TXD) ───────────────────────────────────────────────────► GP1 (RX)     │
  │  GPIO15 (RXD) ◄─────────────────────────────────────────────────── GP0 (TX)     │
  │  GND ─────────────────────────────────────────────────────────────── GND        │
                                                                          Pico ◄──────┘
                                                                    pico-seplos-emu firmware
                                                             (SeplosResponder, canned telemetry)
```

Contrast with the software-only path (`run-bin-rpi` skill): there, `bin-rpi` talks to a
pty-backed `test-kit::SeplosSim` in the same process tree, no real UART involved. This bench
replaces that pty leg with a real wire and real timing, keeping everything else (config shape,
MQTT pipeline, topic names) identical.

## Flash instructions

No debug probe (JTAG/SWD) needed — this firmware is a fixed, deterministic emulator, not code
under active on-target debugging.

1. Install the flash tool once: `cargo install elf2uf2-rs`.
2. Hold the Pico's **BOOTSEL** button, plug it into USB, release BOOTSEL — it mounts as a mass
   storage device.
3. From `hil/pico-seplos-emu/`: `cargo run --release`. The crate's `.cargo/config.toml` runner
   (`elf2uf2-rs --deploy --serial --verbose`) converts the built ELF to a `.uf2` and copies it to
   the mounted drive; the Pico reboots into the new firmware automatically.

(Verified against `/mnt/d/hvd/embassy-rp2040-template`, a working embassy-rp project on this
machine — same runner line, same no-probe convention.)

## Smoke test procedure

1. **Confirm the real device node** on your bench Pi 5 — don't trust `docs/deployment.md` §2.1's
   table blindly (it's flagged provisional and predates any Pi 5-specific check, since the Pi 5's
   RP1 I/O chip changed UART mux behavior vs. Pi 4):
   ```sh
   ls -la /dev/tty*        # before enabling the UART overlay
   # enable the overlay per docs/deployment.md §2.1 (e.g. dtoverlay=disable-bt for GPIO14/15)
   ls -la /dev/tty*        # after — diff to find the new node
   ```
   If it doesn't match `/dev/ttyAMA0`, update `hil/sentient-bench.toml`'s `[serial].port` (or
   override via `SENTIENT_SERIAL__PORT=...` without editing the file) — and if `docs/deployment.md`
   §2.1's table turns out wrong for Pi 5, fix that doc with the corrected finding.

2. **Flash the Pico** per [Flash instructions](#flash-instructions) above, wire it up per
   [Wiring](#wiring), then run bin-rpi against it:
   ```sh
   SENTIENT_CONFIG=hil/sentient-bench.toml cargo run -p bin-rpi
   ```
   This is the `run-bin-rpi` skill's documented "human path" — its Python pty-driver script does
   NOT apply here (this is real hardware, not the pty simulator); only its stale-process/
   port-already-bound gotcha still applies (check nothing else already holds the broker port).

3. **Assert the pipeline is live** — subscribe to the bench device's topics and watch for the
   telemetry pair every poll cycle:
   ```sh
   mosquitto_sub -h 127.0.0.1 -p 1883 -t 'tms/HIL-BENCH-001/#' -v
   ```
   Expect exactly two messages per poll cycle: `tms/HIL-BENCH-001/dc_measurement_update` and
   `tms/HIL-BENCH-001/power_hardware_update` — the same two topics
   `crates/test-e2e/tests/e2e_l1.rs`'s `good_telemetry_publishes_two_tms_messages_matching_snapshot`
   asserts against the pty sim, just with the bench device id. If `bin-rpi`'s `Health` counters are
   reachable in your run, confirm `telemetry_ok` increments and `telemetry_err` stays 0 across
   several `poll.interval_ms` cycles.

   This is a **manual/occasional bench check, not a CI gate** — insta-style snapshot matching
   doesn't apply here (real hardware timing jitter isn't snapshot-stable the way the in-process
   pty harness is).

## Phase 2 (forward-compat — designed, not built)

- **A second responder node** (e.g. an ESP32) plugs into UART1 (GP4=TX, GP5=RX on the Pico —
  reserved/unused in phase 1, see `pico-seplos-emu/src/main.rs`'s module doc) without touching the
  UART0 wiring or the responder protocol logic at all.
- **An RS-232/RS-485 electrical leg** (MAX3232/MAX3485 breakout), needed only when benching against
  a *real* pack's electrical interface rather than another emulator, is a second implementation of
  `pico_seplos_emu::transport::UartChannel` — the seam `impl_embassy_uart0.rs` already implements
  for the raw-TTL case. `pico-seplos-emu-core`'s protocol logic never needs to change for either.

## Known follow-up (out of this plan's scope)

`lib-parse`'s `Cargo.toml` carries an unconditional `bytes = { workspace = true }` dependency that
is dead code (zero `bytes::` usages anywhere in `lib-parse`/`lib-canonical` — verified via
`grep -rn "bytes::" crates/lib-parse/src crates/lib-canonical/src`) and blocks
`cargo check -p lib-parse --no-default-features --features seplos --target thumbv6m-none-eabi`
(RP2040/Cortex-M0+ has no LDREX/STREX — no native atomic RMW — and `bytes`'s ref-counting needs
`AtomicUsize::fetch_add`/`compare_exchange`). Feature-gating or dropping that dependency would let
`pico-seplos-emu-core`'s vendored `checksum.rs` become a real `lib-parse` dependency instead of a
deliberate byte-identical copy. One-line fix, low risk, out of scope for this bench-tooling PR.

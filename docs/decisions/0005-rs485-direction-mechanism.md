# 0005 — RS-485 direction-control mechanism: a config seam, RTS+drain default

**Status:** Locked (2026-07-16). Field task open (see Consequences).

## Context

`docs/gap-analysis-meteor-parity.md` flags this as **"the one finding with field-hardware
risk"**, independently raised by both the serial-transport and protocols analysts. The legacy
Python prototype uses the kernel's `TIOCSRS485` (hardware-timed driver-enable) for passive reads
and a dedicated GPIO path (`gpiochip4` pin 22, 5 ms/10 ms settle delays) for the Seplos poller.
Before this workstream, sentient-rs only toggled RTS in software around `write_all`
(`lib-io/src/host/impl_rs485.rs`), with **no drain/settle delay** — risking a truncated frame tail
if the OS write returns before the UART shift register drains — and could not drive a bus at all
if the transceiver's DE/RE line was wired to GPIO rather than RTS (as the baseline's Pi 5 HAT setup
was). A stale doc comment (`lib-io/src/host/mod.rs:5`, since fixed) falsely claimed `TIOCSRS485`
was already in use.

Three real mechanisms exist on Linux for asserting a transceiver's driver-enable line during a
half-duplex RS-485 write, each with different failure modes if mismatched against the actual
wiring:

1. **Software RTS toggling** — the driver flips RTS itself around the write, with an explicit
   drain (`flush` + `tcdrain`) before dropping it, plus configurable pre/post settle delays. Works
   on any UART, but relies on software timing being *good enough* — the OS's `write()` returning
   doesn't guarantee the UART's shift register has actually finished transmitting, hence the drain
   step.
2. **Kernel `TIOCSRS485`** — the kernel driver hardware-times the DE assertion itself, once
   configured at `open()`; after that, `set_direction` calls become a no-op since the
   driver/hardware owns toggling. Removes the software-timing risk entirely, but needs kernel/UART
   driver support for the ioctl.
3. **Dedicated GPIO DE/RE** — for HATs that don't wire DE to RTS at all, a separate GPIO line must
   be driven directly (via `gpiocdev`), with its own settle delays.

## Decision

**Ship all three as a single, per-port config seam, with software-RTS-plus-drain as the default —
not a single hardcoded mechanism.** `lib_io::host::DirectionMode` (issue #245) is a three-way enum
(`RtsSoftware{drain, settle_tx_us, settle_rx_us}` / `KernelRs485` / `GpioDe`) selected at
`HostSerial::open_with_direction`; `SerialConfig.direction` (`"rts"` default / `"kernel"`|
`"tiocsrs485"` / `"gpio"`) is the per-`[serial]`/`[[bus]].serial` TOML selector, with
`gpio_chip`/`gpio_line`/`gpio_active_high`/`settle_tx_us`/`settle_rx_us`/`tx_drain` as its
supporting fields. The default closes the concrete truncation-risk bug the gap analysis flagged:
RTS mode now always drains (`flush` + `tcdrain`) before deasserting DE, with generous default
settle delays (5 ms pre-write, 10 ms post-write — matching the baseline's own GPIO-path timing),
rather than the prior no-drain, no-settle software toggle.

## Consequences

- **A device wired for GPIO DE/RE (like the baseline's own Pi 5 HAT) is no longer unreachable** —
  `direction = "gpio"` plus `gpio_chip`/`gpio_line` drives it directly, closing the "not driving
  the bus at all" failure mode the gap analysis called out.
- **A device on a kernel-`TIOCSRS485`-capable UART gets hardware-timed correctness** without
  relying on software drain/settle tuning at all — the more robust option where available.
- **The false doc comment is fixed**: `lib-io/src/host/mod.rs` now accurately describes the
  three-mode seam instead of falsely claiming `TIOCSRS485` was already in use.
- **Field verification is explicitly still owed, not closed by this decision.** Per
  CLAUDE.md's Embedded Engineering Rules ("test on the host before the target"), the drain/settle
  *sequencing* for `RtsSoftware`'s receive side is pty-provable and is covered by CI
  (`lib-io/tests/direction_mode.rs`), but a pty implements none of the modem-control ioctls
  `write_request_to_send`/`TIOCSRS485`/GPIO toggling actually need — **none of the three modes has
  a real "it correctly asserts DE on real silicon" proof yet.** The field task this decision
  carries forward: verify all three mechanisms against the actual shipping Waveshare HAT (see
  `docs/deployment.md` §2.1) before enabling anything other than the RTS-software default in a
  production config.
- `SerialConfig::direction`/the top-level `[direction]` section and `bin_rpi_platform::
  DirectionMode` (moved from `impl_app::host` under ADR 0009) currently carry the config surface
  end-to-end (`DirectionCfg::to_host_direction` maps into it) but `HostPlatform::build_port` does
  not yet thread it through to `HostSerial::open_with_direction` — the config seam exists; the
  last wiring hop into the actual open call is a follow-up integration pass (see `SerialConfig`'s
  and `bin_rpi_platform::DirectionMode`'s own doc comments for the exact STUB boundary).

## Revisit trigger

- Field verification against the shipping HAT completes (positively or negatively) — either
  confirms the current default is correct for the shipping hardware (record supersedes to
  "verified"), or surfaces a mismatch that needs a different default/mechanism.
- The remaining `HostPlatform::build_port` → `HostSerial::open_with_direction` wiring lands,
  converting this from "config seam exists" to "fully live end-to-end."

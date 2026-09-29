# 0010 — MCU runtime: pivot the Cortex-M target from Zephyr to Embassy

**Status:** Accepted (2026-07-24) — EPIC #350, story S6 (#357)

## Context

The MCU target originally ran **Zephyr** (`no_std`) with an embassy-on-Zephyr executor, a C
UART shim, and a hand-rolled `block_on`. That path shipped as `bin-zephyr` (a `west` app,
excluded from the cargo workspace) plus the `bin-zephyr-platform` adapter crate. Two forces made
it the wrong foundation:

- **The Pi already left Zephyr.** ADR-adjacent decision #159 (2026-07-12, see
  [`architecture.md`](../architecture.md) §2) moved the Pi to a minimal **Yocto Linux** image
  running the `std`/tokio `bin-rpi` binary. That reversal left Zephyr owning exactly **one**
  target — the Cortex-M MCU — so the RTOS was carrying its weight for a single consumer.
- **The seam side-steps cost real time.** The Zephyr adapter drove raw `zsock_*` from the pump
  behind a `block_on` that never actually suspended (faking async over blocking C), bypassing
  Zephyr's socket semantics *and* the `SerialTransport`/`Publisher` abstraction at once (see the
  *Abstraction & Platform Seams* doctrine in `CLAUDE.md`). The RTOS was not buying us isolation
  or portability we were actually using; it was buying an emulator-only debugging loop.

Meanwhile a **proven Embassy template already existed in-tree**: `hil/pico-seplos-emu` is an
excluded-workspace RP2350 crate that builds and links against `embassy-rp 0.10` (`rp235xa`
feature, IMAGE_DEF auto-generated), `embassy-executor 0.10`, `embassy-time 0.5.1`, on
`thumbv8m.main-none-eabihf`. It is the verified reference for the toolchain, the memory map, and
the boot-block placement. Adopting Embassy for the shipping MCU binary reuses that template
rather than re-deriving it.

The runtime is reached only through the `spec_app::Platform` port (ADR
[0009](./0009-spec-app-ports-and-adapters.md)) — `transport()`, `publisher()`, `now()`,
`sleep()`, … — so swapping the executor is an adapter swap, not a rewrite of the poll → map →
publish policy in `impl-app`.

## Decision

Retire Zephyr on the MCU and adopt **Embassy** as the Cortex-M runtime. The Pi is unchanged
(Yocto Linux / `std` / tokio — this ADR does not touch it).

1. **New workspace member `crates/bin/mcu-platform`** — the Embassy MCU platform adapter,
   implementing `spec_app::Platform`, the symmetric `no_std` peer of `bin-rpi-platform`. It is
   deliberately **embassy-version-free**: `now()`/`sleep()` go through a small injected `McuClock`
   seam rather than a direct `embassy-time` dependency, so the crate stays host-clippy-able and
   avoids the `embassy-time` `links`-key version conflict (the workspace `lib-net` embassy spike
   pins one version; the shipping bin pins another). The concrete embassy-time-backed clock is
   supplied by the bin crate.
2. **New excluded workspace `crates/bin/mcu-rp2350`** — the shipping MCU binary, modelled directly on
   `hil/pico-seplos-emu`: its own `Cargo.lock`, its own pins (`embassy-rp 0.10` `rp235xa`,
   `embassy-executor 0.10`, `embassy-time 0.5.1`, `embassy-sync 0.6.2` for the channel halves),
   its own `.cargo/config` pinning `thumbv8m.main-none-eabihf`. It is **never** built by
   `cargo build --workspace` from the repo root; CI builds it explicitly so path-dep drift breaks
   loudly. Member `ns` (non-secure world) is the telemetry binary; `s`/`abi` join it with the
   S/NS split (ADR [0011](./0011-mcu-security-boundary.md)).
3. **`lib-net`'s embassy spike is now a consumer path, not a shelf item.** The previously
   "consumed by no crate" `lib-net/src/embassy/` code (channel halves over `embassy-sync 0.6.2`,
   the device-hosted broker pump, the `LatestStore`) is now linked by `bin/mcu-rp2350/ns`.
4. **The Zephyr tree is deleted in the same change** — `bin/zephyr/**`, the `bin-zephyr-platform`
   crate, and the Zephyr CI lanes (`_release-zephyr.yml`, `_emulation.yml`, `.ci/zephyr.Dockerfile`,
   the `production.yml` `mcu-boot-smoke` job, the justfile `lint-zephyr`/qemu-e2e recipes). Nothing
   green is lost — those jobs were already skipped (#191). Git history is the archive; the trait
   seams make the reversal mechanical if ever needed (see revisit trigger).
5. **Demo silicon is RP2350 A4 stepping only.** Earlier steppings have documented errata that this
   scaffold does not work around.

### Verification is build/link + HIL smoke — there is **no RP2350 emulation**

Stated honestly, per the `CLAUDE.md` "don't invent external-tool facts" doctrine: **Renode and
QEMU have no RP2350 model.** There is therefore no emulated boot/run gate for this binary. What is
gated:

- `cargo build --release --target thumbv8m.main-none-eabihf` for `ns` (± `core1-parser`) **links**.
- `just build-no-std` covers the shared `no_std` crates on both the `thumbv7em` roster and the new
  `thumbv8m` line.
- On-target confirmation is **HIL smoke on the bench Pico 2** (`sentient-pi-bench`, SWD via
  linuxgpiod, RTT over OpenOCD) — a queued follow-up story, **not** a merge gate. The procedure is
  captured in [`../perf/rp2350-footprint-perf.md`](../perf/rp2350-footprint-perf.md) (Performance §,
  HIL-gated) and `docs/testing/mcu-dds-spdp-hil.md`.

The build+link gate proves the SAU-config / veneer / handoff logic *compiles and links*; it does
**not** demonstrate the image is boot-ROM-loadable on real silicon. That gap is on the
hardware-bring-up ledger, called out rather than assumed away.

## Consequences

- **One runtime, one target, reused template.** The MCU stack is now `embassy-rp` all the way down
  from the same verified base as `hil/pico-seplos-emu`; no RTOS to configure, no C `block_on` to
  fake async over.
- **`bin-zephyr-platform` → `bin-mcu-platform`.** Every architecture reference to the retired
  Zephyr adapter now names the Embassy adapter; the dependency graph in
  [`architecture.md`](../architecture.md) §5 is updated accordingly.
- **The seam discipline held.** Because `impl-app` was already written against `spec_app::Platform`,
  the pivot changed zero lines of poll/map/publish policy — only which adapter links.
- **Bootloader is deferred and Gate-0-blocked.** This ADR ships the *runtime*, not a secure-boot
  chain. The bootloader (and the wolfCrypt-FIPS-via-FFI crypto module behind it) is blocked on the
  Gate-0 tracking issue until the written FIPS-scope / CNSA / attestation / OE answers land. Signed
  boot is a **claim of the security architecture** (ADR [0011](./0011-mcu-security-boundary.md)),
  not something demonstrated by this runtime pivot.
- **Unused embassy pins pruned.** The never-built `embassy-stm32`/`embassy-nrf`/`embassy-rp`
  workspace pins (and the matching `lib-io` `stm32`/`nrf`/`rp2040` features) are deleted to kill the
  lockstep-bump treadmill; the excluded `bin/mcu-rp2350` workspace owns its own MCU pins.

## Revisit trigger

- **A written answer to the Gate-0 "bare-metal-vs-RTOS operating-environment" question could
  reopen Zephyr.** If the FIPS/attestation operating-environment designation turns out to *require*
  an RTOS OE (e.g. a validated-module OE listed against an RTOS rather than bare-metal Embassy),
  the runtime decision is back on the table. The trait seams (`spec_app::Platform`) are the exact
  mechanism that keeps that reversal cheap — a Zephyr adapter would be a new `bin-*-platform` peer,
  not a rewrite.
- **A second MCU target appears** (e.g. the single-core STM32H5 the boundary architecture is
  designed to also serve) — validates or stresses the single Embassy adapter shape.
- **RP2350 emulation becomes available** (a Renode/QEMU model lands) — at which point an emulated
  boot-smoke gate can replace the "build/link only" posture above.

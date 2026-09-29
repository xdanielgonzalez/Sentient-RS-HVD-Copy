<!-- rtk-instructions v2 -->
# RTK (Rust Token Killer) - Token-Optimized Commands

## Golden Rule

**Always prefix commands with `rtk`**. If RTK has a dedicated filter, it uses it. If not, it passes through unchanged. This means RTK is always safe to use.

**Important**: Even in command chains with `&&`, use `rtk`:
```bash
# ❌ Wrong
git add . && git commit -m "msg" && git push

# ✅ Correct
rtk git add . && rtk git commit -m "msg" && rtk git push
```

## RTK Commands by Workflow

### Build & Compile (80-90% savings)
```bash
rtk cargo build         # Cargo build output
rtk cargo check         # Cargo check output
rtk cargo clippy        # Clippy warnings grouped by file (80%)
rtk tsc                 # TypeScript errors grouped by file/code (83%)
rtk lint                # ESLint/Biome violations grouped (84%)
rtk prettier --check    # Files needing format only (70%)
rtk next build          # Next.js build with route metrics (87%)
```

### Test (60-99% savings)
```bash
rtk cargo test          # Cargo test failures only (90%)
rtk go test             # Go test failures only (90%)
rtk jest                # Jest failures only (99.5%)
rtk vitest              # Vitest failures only (99.5%)
rtk playwright test     # Playwright failures only (94%)
rtk pytest              # Python test failures only (90%)
rtk rake test           # Ruby test failures only (90%)
rtk rspec               # RSpec test failures only (60%)
rtk test <cmd>          # Generic test wrapper - failures only
```

### Git (59-80% savings)
```bash
rtk git status          # Compact status
rtk git log             # Compact log (works with all git flags)
rtk git diff            # Compact diff (80%)
rtk git show            # Compact show (80%)
rtk git add             # Ultra-compact confirmations (59%)
rtk git commit          # Ultra-compact confirmations (59%)
rtk git push            # Ultra-compact confirmations
rtk git pull            # Ultra-compact confirmations
rtk git branch          # Compact branch list
rtk git fetch           # Compact fetch
rtk git stash           # Compact stash
rtk git worktree        # Compact worktree
```

Note: Git passthrough works for ALL subcommands, even those not explicitly listed.

### GitHub (26-87% savings)
```bash
rtk gh pr view <num>    # Compact PR view (87%)
rtk gh pr checks        # Compact PR checks (79%)
rtk gh run list         # Compact workflow runs (82%)
rtk gh issue list       # Compact issue list (80%)
rtk gh api              # Compact API responses (26%)
```

### JavaScript/TypeScript Tooling (70-90% savings)
```bash
rtk pnpm list           # Compact dependency tree (70%)
rtk pnpm outdated       # Compact outdated packages (80%)
rtk pnpm install        # Compact install output (90%)
rtk npm run <script>    # Compact npm script output
rtk npx <cmd>           # Compact npx command output
rtk prisma              # Prisma without ASCII art (88%)
```

### Files & Search (60-75% savings)
```bash
rtk ls <path>           # Tree format, compact (65%)
rtk read <file>         # Code reading with filtering (60%)
rtk grep <pattern>      # Search grouped by file (75%). Format flags (-c, -l, -L, -o, -Z) run raw.
rtk find <pattern>      # Find grouped by directory (70%)
```

### Analysis & Debug (70-90% savings)
```bash
rtk err <cmd>           # Filter errors only from any command
rtk log <file>          # Deduplicated logs with counts
rtk json <file>         # JSON structure without values
rtk deps                # Dependency overview
rtk env                 # Environment variables compact
rtk summary <cmd>       # Smart summary of command output
rtk diff                # Ultra-compact diffs
```

### Infrastructure (85% savings)
```bash
rtk docker ps           # Compact container list
rtk docker images       # Compact image list
rtk docker logs <c>     # Deduplicated logs
rtk kubectl get         # Compact resource list
rtk kubectl logs        # Deduplicated pod logs
```

### Network (65-70% savings)
```bash
rtk curl <url>          # Compact HTTP responses (70%)
rtk wget <url>          # Compact download output (65%)
```

### Meta Commands
```bash
rtk gain                # View token savings statistics
rtk gain --history      # View command history with savings
rtk discover            # Analyze Claude Code sessions for missed RTK usage
rtk proxy <cmd>         # Run command without filtering (for debugging)
rtk init                # Add RTK instructions to CLAUDE.md
rtk init --global       # Add RTK to ~/.claude/CLAUDE.md
```

## Token Savings Overview

| Category | Commands | Typical Savings |
|----------|----------|-----------------|
| Tests | vitest, playwright, cargo test | 90-99% |
| Build | next, tsc, lint, prettier | 70-87% |
| Git | status, log, diff, add, commit | 59-80% |
| GitHub | gh pr, gh run, gh issue | 26-87% |
| Package Managers | pnpm, npm, npx | 70-90% |
| Files | ls, read, grep, find | 60-75% |
| Infrastructure | docker, kubectl | 85% |
| Network | curl, wget | 65-70% |

Overall average: **60-90% token reduction** on common development operations.
<!-- /rtk-instructions -->

## Code Organization

**Type definitions and their `impl` blocks live in SEPARATE, prefixed files.** This is a deliberate Go-style separation of data from behavior — we come from a Go world and follow it here. It applies to **every** source file — **no exclusions**, no matter how small.

- **Type files** — a type definition (`struct` / `enum` / `trait` / type alias) goes in its **own file, one primary type per file, colocated under a `types/` sub-module** (`foo/types/{mod.rs, a.rs, b.rs, …}`) so types stay together and the module directory doesn't bloat. **No `type_` prefix** — the `types/` module already marks these as type definitions, so the prefix is redundant (`foo/types/broker_core.rs`, not `type_broker_core.rs`). This holds for **every** split — including pure-data DTO catalogues (each DTO gets its own file under `types/`). (`impl_*.rs` files DO keep their prefix — they sit at the module root next to `mod.rs`, so the prefix is what distinguishes them.)
- **Impl files** — a type's implementations (`impl Foo { … }`, `impl SomeTrait for Foo { … }`) go in **`impl_<name>.rs`** at the **module root**, grouped by the type being implemented — kept lean and separate from the type definitions.
- **Re-export** everything from `types/mod.rs` and the module's `mod.rs` so public paths are unchanged (the split is purely internal / behavior-preserving).
- A file that mixes a type with its impl — or holds multiple types (a "god module") — is split into a directory module: `foo/{mod.rs, types/{mod.rs, a.rs, b.rs, …}, impl_a.rs, impl_b.rs, …}`.
- **Massive files also sub-split the BEHAVIOR, not just type-vs-impl.** A large god-module / state machine (e.g. a 1500-line `mqtt_server.rs`) becomes a module whose impls are further split by concern (state-machine phase, feature area) — one giant `impl_broker_core.rs` is itself a smell. Size routes the work: small files are mechanical; massive ones need a thoughtful decomposition.
- **Imports — type files import DIRECTLY; a module `prelude` is for `impl_*` files ONLY.** A `type_*.rs` file must **never** do `use super::super::prelude::*` — that reaches up into the module's `prelude.rs`, which does `pub use super::types::*` back down, forming a `types ⇄ prelude` glob **cycle** (fragile; trips unused-glob and re-export-visibility lints). Each type file imports **exactly** what it references: a sibling type as `use super::a::X;` (sibling file in the same `types/`), externals by full path (`use crate::error::Result;`, `use rustls::sign::CertifiedKey;`). Only the `impl_*.rs` files (at the module root) use `use super::prelude::*` — **one** level up, which is acyclic because impls aren't members of `types/`. The per-module private `prelude.rs` therefore holds `pub use super::types::*` **+** the externals the impls share, and it must **`pub use`** (not private `use`) those externals or the impl glob re-exports nothing. `types/mod.rs` re-exports each type at its own visibility (`pub(in crate::foo) use a::*;` for a `pub(in …)`-restricted type — a plain `pub use` over-claims and warns under `-D warnings`).

**Enforced by a smell-test gate** (stage-1 `just lint` / `just smell` / CI): `.ci/scripts/typeimpl_audit.py --check` fails on any **new** type-plus-impl-in-one-file or multiple-types-in-one-file, baseline-ratcheted via `.ci/scripts/typeimpl_baseline.txt` (the existing violations are worked down by the reorg; run `just smell-baseline` to shrink it as splits land). `#[cfg(test)]` module helper types are exempt.

**Verify** every split is behavior-preserving: `rtk cargo build`, `rtk cargo test`, and `rtk cargo clippy` stay green; for `no_std` crates, the `thumbv7em-none-eabihf` target still links.

**Example:** `crates/impl-app/src/zephyr.rs` (types + impls for the UART transport, spin-arena, publishers, monitor logger, broker pump, platform factory) → `zephyr/{mod.rs, types/{mod.rs, zephyr_uart.rs, spin_arena.rs, zephyr_publisher.rs, …}, impl_zephyr_uart.rs, impl_spin_arena.rs, impl_zephyr_publisher.rs, …}`, re-exported up through `mod.rs`.

## Platform & Deployment — Pi ships Yocto Linux, MCU ships Embassy (RP2350)

**The two shipping targets run different runtimes: the Raspberry Pi (4-64 **and** 5) ships a minimal **Yocto Linux** image (`core-image-minimal` + systemd) running the `bin-rpi` std/tokio binary; the MCU ships **Embassy** on the **RP2350** (Cortex-M33, `no_std`) as `crates/bin/mcu-rp2350` + the `crates/bin/mcu-platform` adapter.** This is the result of **two separate pivots away from Zephyr**, and they must not be conflated: (1) the **Pi** left Zephyr for Yocto Linux on 2026-07-12 (story #159 under EPIC #6; full plan in `~/.claude/plans/glittery-frolicking-peach.md`) — the rationale below is that decision's, and it is history, not a live choice; (2) the **MCU** then left Zephyr for Embassy on 2026-07-24 (ADR [0010](docs/decisions/0010-mcu-runtime-embassy.md), EPIC #350) — `bin/zephyr` and `bin-zephyr-platform` are deleted, so **no Zephyr remains in the tree on either target**. Rationale + consequences of the Pi pivot:

- **The pivot DISSOLVES the RPi-networking blocker.** The Pi could not run Zephyr networking: no Ethernet L2 driver exists for the Pi 5 (no in-tree Broadcom/GENET, the GbE MAC lives in the PCIe-attached RP1 chip, no devicetree MAC node) — so both `zsock` and an owned smoltcp stack were stuck, both needing L2 frames nobody drives. **Running Linux removes this entirely:** the mainline kernel's `bcmgenet`/RP1 drivers own L2, and the existing tokio/`std::net` path in `bin-rpi` just works. **Consequence:** the owned embassy-net/smoltcp `NetStack` + `lib-net-embassy` are now **MCU-only**; on the Pi, `std`/tokio is the **deployment** path, not merely a host-test seam.
- **This is packaging + doctrine + CI, NOT a Rust redesign.** `bin-rpi` was already a shipping std/tokio Linux binary (signed, SBOM'd, systemd `sd-notify`, embedded rumqttd broker, tokio-serial, mosquitto bridge). The `Platform`/`SerialTransport`/`Publisher` seams are unchanged — the existing **host impl is now the Pi shipping impl** (as well as the `cargo test` impl); the separate `no_std` impl stays the MCU one (then `bin-zephyr-platform`, now `bin-mcu-platform` — the ADR-0010 swap changed which adapter links, not the seams).
- **Attack-surface posture (stated honestly).** Zephyr-on-Pi was chosen to shrink attack surface vs. Raspbian. Yocto `core-image-minimal` (no package manager, no SSH, read-only rootfs, hardened systemd) **plus a first-image hardening pass** (image minimization, systemd sandbox, KSPP kernel, nftables + mosquitto TLS, cve-check/SBOM gates — task #154) is the **compensating control**. **Residual risk, accepted + documented:** the Pi's closed VideoCore BootROM ⇒ no fully-open/auditable root of trust — a real posture delta vs. the Zephyr baseline.
- **Decisions (locked):** both Pi 4-64 + Pi 5 (one aarch64 **musl-static** `bin-rpi` serves both, ABI-independent of the image glibc); the **prebuilt signed** binary is packaged into the image (no Rust toolchain in BitBake); stock kernel first (PREEMPT_RT deferred — the telemetry poll loop is soft-real-time); keep **two build pipelines** (originally `west` for the MCU; since ADR 0010 the MCU pipeline is `crates/bin/mcu-rp2350`, its own **excluded cargo workspace** with its own `Cargo.lock` and embassy pins, built by `just mcu-rp2350` — the meta-zephyr `BBMULTICONFIG` convergence question is moot, not deferred); pin Yocto **wrynose (5.0 LTS)**; all Yocto content lives under a top-level **`distro/`** folder (distinct from `crates/`).
- **The MCU path was unchanged by the Pi pivot** (it changed later, on its own, via ADR 0010). Everything in *Abstraction & Platform Seams* and *Embedded / Firmware Engineering Rules* below still governs the MCU `no_std` side — the owned-stack contingency, the seam discipline, the SRAM budget. Those rules were written against Zephyr and the QEMU Cortex-M loop; the **doctrine still holds**, but read the Zephyr-specific examples in them as history — and note there is **no RP2350 emulation** (neither Renode nor QEMU models the chip), so the MCU gate is build/link plus bench HIL, not an emulator run.

## Abstraction & Platform Seams — program against the trait, not the raw platform

This codebase is dual-target (Pi = Yocto Linux / std+tokio, MCU = Zephyr / `no_std` — see *Platform & Deployment* above) **by construction**: platform-specific I/O sits behind trait seams (`SerialTransport`, `Publisher`, `Platform`, `BrokerListener`/`BrokerConn`, and the owned `NetLink`/`NetStack` path) with a **host/std impl** (the Pi shipping impl, and the `cargo test` impl) **+** the Zephyr target impl (the MCU shipping impl). Reaching for a raw platform call (`zsock_*` / FFI / a C shim) inside app logic is the SIGNAL to define or extend a seam — **not** to bypass one.

**The tells of a side-step** (each cost real time + money on the T3 broker): raw `zsock_*` calls driven straight from the pump; a `block_on` that doesn't actually suspend (faking async over blocking C); hand-rolled userspace backpressure/retry that a correctly-*configured* platform owns. The result bypassed Zephyr's socket semantics, embassy's async HAL, **and** the `SerialTransport`/`Publisher` abstraction all at once — un-host-testable, and it burned hours fighting the net stack instead of configuring it.

- **New platform/transport/IO goes behind a trait seam.** The target's raw calls are ONE impl of the seam; app logic programs against the trait. Always ship a host impl (std/tokio or a mock) so the logic is `cargo test` / Miri-able **without the emulator**.
- **Keep the FFI/C shim DUMB** — one-shot passthroughs (a single `send`/`recv`), no loops, retry, or policy. Backpressure, framing, and retry belong to a correctly-configured platform or to host-testable Rust behind the seam — never buried in a C shim.
- **Configure the platform before working around it.** A blocking-socket `ENOMEM` means the stack is under-provisioned — fix the Kconfig, don't spin in userspace. Working around a misconfig in a shim is the side-step.
- **If the seam doesn't exist yet, CREATE it** (mirror an existing one, e.g. `SerialTransport`) rather than inlining raw calls "just for now" — the "just for now" *is* the debt.

## Embedded / Firmware Engineering Rules

Hard-won on the Zephyr MCU path (`bin-zephyr` / `impl-app` / `lib-*`; 64 KB SRAM). Follow these — each one cost real debugging time to learn.

**Test on the host before the target.** Push logic into pure, host-testable crates (`lib-*`, `impl-app` core); keep the firmware/FFI layer thin. Reproduce bugs on the host FIRST — `cargo test`, **Miri** (`-Zmiri-many-seeds`; catches UB / OOB / `&mut` aliasing / data races), **Loom** (lock-free/atomic code), guard-byte fuzz. The QEMU/Renode run is the *final confirmation*, not the debugging loop: chasing a fault through emulator rebuild cycles burns hours; a host checker pinpoints it in seconds. If an agent is guess-rebuilding on the emulator, stop it and move the repro to the host.

**Memory model — store canonical, render on output.** On the MCU, do NOT store the serialized/wire form (JSON is ~10× inflated, and multiple stored copies blow the RAM budget). Keep compact canonical data (`CanonicalReading`) in memory (a bounded ring-buffer arena); map → TMS → JSON only at transmission, into ONE reused buffer, then forget. The broker stores no payload bytes and renders on demand.

**Zero-copy / stack discipline.** Never materialize large buffers or structs as per-call stack locals on a hot path — put them in `static`/reused storage (single-consumer thread, `unsafe` + safety comment), one reused scratch, not per-iteration allocations. Note Rust reserves the **max frame across all `match` arms** in a function, so a cheap arm pays for an expensive sibling's frame — split divergent-cost arms into `#[inline(never)]` fns (e.g. the `emit_reading` Broker-vs-RemoteClient split dropped its frame 1712 B → 16 B).

**Size stacks/buffers by MEASUREMENT, not guessing.** Use `RUSTFLAGS=-Zemit-stack-sizes`, `CONFIG_THREAD_ANALYZER` / stack high-water marks, and PSP-vs-linker-map correlation. On a full board, growing a stack is the *wrong* lever — fix the frame-size / data-model cause. When you must grow one, identify the *actual* overflowing thread (via `CONFIG_HW_STACK_PROTECTION` + `CONFIG_MPU_STACK_GUARD`) — don't bump the wrong thread's stack.

**Concurrency on `static` state.** Guard shared `static mut` with ONE spinlock whose critical section covers the *whole* access; copy/clone owned data OUT under the lock so no borrow escapes it. `heapless::spsc` is single-producer-single-consumer only — never add a second producer (even spinlock-serialized) without proving it under Loom/Miri.

**Cortex-M fault triage.** `Illegal use of EPSR`, `Illegal load of EXC_RETURN into PC`, or `PC = 0xaaaaaaaa` (stack-poison fill) mean memory/stack corruption or a clobbered return address — NOT a logic bug. Inspect corrupted register *content* for the source (e.g. ASCII-digit bytes ⇒ a JSON/text buffer overran). Enable `HW_STACK_PROTECTION` + `MPU_STACK_GUARD` + `EXTRA_EXCEPTION_INFO` to disambiguate stack-overflow from cross-thread corruption before changing anything.

**Don't invent external-tool facts.** For Renode / QEMU / Zephyr / zephyr-lang-rust / Seplos details (emulated-NIC capability, board support, Kconfig symbols, monitor commands — all version-dependent), verify against official docs or source and cite them; dispatch a research agent rather than relying on recall. E.g.: Renode's emulated NICs often can't move real frames; `zephyr-lang-rust` supports only a fixed board allow-list (not native_sim); QEMU `qemu_cortex_m3` is the substrate that has both a working timer and a wire-capable NIC.

## Bounded Debugging — get the datum, don't guess

When a bug resists repeated hypothesis-testing — round after round of "is it *X*?", each answered "no" — STOP guessing and capture the **definitive datum** that names the cause directly. Convert an open-ended "rule out suspects" hunt (unbounded, slow) into a bounded "get one ground-truth fact, then fix that one thing."

- Reach for an **oracle** — a tool that returns a *fact*, not a hypothesis:
  - **Symbolized fault dump** — `addr2line` the faulting PC / LR / return-chain against the (unstripped) ELF → names the exact function + line. (Poison/garbage PC? Symbolize LR and the stacked return addresses instead.)
  - **Miri / Loom** — proves or disproves UB, OOB, `&mut` aliasing, and data races deterministically (run Miri on the target's pointer width too, e.g. `i686` for a 32-bit MCU).
  - **`-Zemit-stack-sizes` + thread stack-analyzer / `-Zmiri-many-seeds`** — exact frame sizes and stack high-water; **guard-byte / canary fuzz** — pinpoints the overflowing write.
- The anti-pattern is N speculative fix→build→run cycles on the emulator; each "ruled out another suspect" is expensive and open-ended. One oracle datum ("it **is** function `Y` overrunning its stack") makes the fix targeted.
- Corollary (see the Embedded rules): reproduce on the **host** with a deterministic checker, and **measure**, *before* you change anything. The emulator / hardware run is the final confirmation — not the search.

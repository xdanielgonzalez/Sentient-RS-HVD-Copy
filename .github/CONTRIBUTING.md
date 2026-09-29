# Contributing to sentient-rs

This is a short pointer, not a full guide — the authoritative conventions live in
[`CLAUDE.md`](../CLAUDE.md) at the repo root. Read that first; it covers, in detail:

- **RTK command prefix** — prefix build/git/test commands with `rtk` (`rtk cargo build`,
  `rtk git commit`, ...). It's a token-optimizing passthrough wrapper, always safe to use, even
  inside `&&` chains.
- **Code organization** — type definitions and their `impl` blocks live in separate files
  (`types/*.rs` vs. `impl_*.rs`), enforced by a baseline-ratcheted smell-test gate
  (`just smell` / `.ci/scripts/typeimpl_audit.py --check`).
- **Platform & deployment** — the Raspberry Pi ships a Yocto Linux image (`bin-rpi`, std/tokio);
  the MCU ships Zephyr (`bin-zephyr` / `bin-zephyr-platform`, `no_std`). New platform/transport/IO
  code goes behind a trait seam (e.g. `SerialTransport`, `Publisher`, `Platform`) with a host/std
  impl, never raw platform calls inlined into app logic.
- **Embedded engineering rules** — test on the host before the target (`cargo test`, Miri, Loom)
  and only confirm on QEMU/Renode last; memory-model and stack-discipline rules for the MCU side.
- **Bounded debugging** — when a bug resists repeated guessing, get a definitive datum (a
  symbolized fault dump, a Miri/Loom result) rather than another speculative fix-and-rebuild cycle.

## Branch / PR flow

1. Branch off `main`.
2. Make your change; keep it scoped to one Task where possible (see issue types below).
3. Run the stage-1 gate locally: `rtk cargo build`, `rtk cargo test`, `rtk cargo clippy`, and
   `just lint` (fmt + clippy + the type/impl smell gate). For `no_std` crates, `just build-no-std`
   must still link against `thumbv7em-none-eabihf`.
4. Open a PR against `main` using the PR template — it links back to the specific gates above.
5. CI (`.github/workflows/ci.yml` and friends) re-runs the same `just` recipes; keep local and CI
   green in the same way (`justfile` is the single source of truth for both).

## Filing an issue

Work is tracked as **Epic → Story → Task**, plus standalone **Bug** reports. Pick the template
that matches the size of what you're proposing:

| Template | Use for |
|---|---|
| `Epic` | A high-level initiative spanning multiple Stories. |
| `Story` | A feature-level chunk of work under an Epic. |
| `Task` | A concrete, single-PR-sized unit of work under a Story. |
| `Bug report` | Something is broken or behaves unexpectedly. |
| `Feature request` | An unscoped idea, not yet broken into an Epic/Story. |
| `Spike / investigation` | A time-boxed research question that needs an answer/decision. |
| `Documentation` | Docs that are wrong, missing, or unclear. |
| `Chore / maintenance` | Dependency bumps, refactors, tech debt, CI/tooling changes. |

Security vulnerabilities should **not** go through a public issue template — see
[`SECURITY.md`](SECURITY.md) for private reporting.

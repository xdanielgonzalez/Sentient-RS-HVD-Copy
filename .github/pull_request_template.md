<!--
Thanks for the contribution. Fill in what's relevant and delete the rest — this template is a
checklist, not a form to fill out verbatim. See CLAUDE.md for the RTK command prefix, the
type/impl file-split convention, and the embedded engineering rules referenced below.
-->

## Summary

<!-- What does this PR do, and why? A sentence or two is fine. -->

## Related issue(s)

<!-- Closes #NNN / Part of #NNN (Task/Story/Epic). Link the parent if this is a Task under a Story. -->

## Type of change

- [ ] Bug fix
- [ ] New feature
- [ ] Refactor / tech debt (no behavior change)
- [ ] Documentation
- [ ] Chore (deps, tooling, CI)
- [ ] CI / build pipeline

## Target(s) affected

- [ ] Raspberry Pi (Yocto Linux / std+tokio, `bin-rpi`)
- [ ] MCU (Embassy / `no_std`, RP2350, `crates/bin/mcu-rp2350` / `crates/bin/mcu-platform`)
- [ ] Host only (shared `lib-*` crate, tests, tooling)
- [ ] CI / `distro/` (Yocto BitBake layer)

## How verified

- [ ] `rtk cargo build`
- [ ] `rtk cargo test`
- [ ] `rtk cargo clippy` (`-D warnings`)
- [ ] `just lint` (fmt + clippy + type/impl smell gate) green
- [ ] For `no_std` crates: `just build-no-std` still links (thumbv7em-none-eabihf reference chip and thumbv8m.main-none-eabihf RP2350A)
- [ ] Miri and/or Loom run where this touches `unsafe`, `static mut`, or lock-free/atomic code
- [ ] Reproduced on the host before touching hardware, per CLAUDE.md "Test on the host before the target" (MCU changes only; there is no RP2350 emulator — HIL smoke on the bench is the on-target confirmation)
- [ ] New/updated tests cover the change

<!-- Notes on verification: what you ran, what passed, anything you couldn't verify and why. -->

## Checklist

- [ ] Type definitions and their `impl` blocks are in separate files per CLAUDE.md "Code Organization" (`.ci/scripts/typeimpl_audit.py --check` / `just smell` clean, or baseline updated via `just smell-baseline`)
- [ ] No secrets, credentials, or private keys committed
- [ ] If dependencies changed: `cargo-deny`/SBOM/CVE implications considered (new deps vetted, licenses compatible)
- [ ] New platform/transport/IO code sits behind a trait seam with a host impl, not raw platform calls (CLAUDE.md "Abstraction & Platform Seams")
- [ ] Docs updated if behavior, config, or a public path changed

## Notes / screenshots

<!-- Anything else a reviewer should know. Logs, fault dumps (symbolized with addr2line), or screenshots welcome. -->

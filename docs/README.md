# Documentation index — sentient-rs

> **This directory is also built as an mdbook site** — `book.toml` at the repo root drives it
> (`just docs-serve` to browse locally, `just docs` for the full build including the rustdoc API
> reference). The chapter tree is [`SUMMARY.md`](./SUMMARY.md); this README is the GitHub-browsing
> index and is not itself a book chapter.

Reference docs for the Sentient METEOR telemetry adapter: a production Rust pipeline that polls a
Seplos BMS over RS-485, maps to TMS (MIL-STD-3071), and publishes over MQTT on a Raspberry Pi comms
module, with an Embassy/RP2350 MCU forward path. Start with the [project README](../README.md) for the
one-paragraph overview.

## Guides

| Doc | What it covers |
|---|---|
| [`architecture.md`](./architecture.md) | End-to-end architecture: the Seplos → parse → TMS → MQTT pipeline, the dual-target (RPi / Embassy MCU) trait seams, the security seam, and the MCU-only owned network stack (built and tested; on-device hardware wiring pending). Data-flow + crate-dependency diagrams. |
| [`architecture/crate-graph.md`](./architecture/crate-graph.md) | The committed crate dependency graph (`crates.svg` / `crates.mmd`) and the layering the CI gate enforces. |
| [`deployment/usage.md`](./deployment/usage.md) | Build/test/run guide: every `justfile` recipe, the kellnr mirror, the `no_std`/`thumbv7em` build, the security tier matrix, running the RPi binary + systemd unit, the RP2350 MCU firmware build, and where CI/release config lives. |
| [`deployment/README.md`](./deployment/README.md) | On-device install: RS-485 wiring, config, the mosquitto relay, the systemd unit, health signals, and the failure-mode runbook. |
| [`deployment/release-lifecycle.md`](./deployment/release-lifecycle.md) | The release pipeline: image build → UAT → auto pre-release → manual promotion / v-tag production, and the CMS signing chain end-to-end. |
| [`security/README.md`](./security/README.md) | Security posture, boot integrity, TLS, and hardware-TPM recommendations (Scope E). |
| [`security/integration-map.md`](./security/integration-map.md) | Where each crypto/auth/cert integration point plugs into the system, and the FIPS / TrustZone boundary design. |
| [`security/signing-and-verification.md`](./security/signing-and-verification.md) | Release signing end-to-end: cosign keyless provenance over `SHA256SUMS`, X.509 CMS key-based per-artifact signing + the on-device `verify-binary.sh` boot gate, and the recipient verification runbook. |
| [`compliance/README.md`](./compliance/README.md) | Field-by-field TMS schema-shape compliance vs MIL-STD-3071. |
| [`compliance/attestation.md`](./compliance/attestation.md) | The attestation pipeline: IDL → codegen → coverage JSON → conftest/Rego policy gate → OSCAL assessment results. |
| [`security/wolfcrypt-backend.md`](./security/wolfcrypt-backend.md) | The wolfCrypt concrete crypto backend (`feature = "wolfcrypt"`) and its separate build ledger. |
| [`testing/seplos-sim.md`](./testing/seplos-sim.md) | The Seplos BMS device simulator (`SeplosSim`/`seplos-tcp-sim`): library API, spec grammar, fault-injection taxonomy, the cucumber harness, and QEMU/Yocto e2e wiring. |
| [`reference/README.md`](./reference/README.md) | Landing page for the rustdoc API reference grafted into the built book (one entry per workspace crate) — also the canonical per-crate module/layer/role table. |
| [`reports/audit-2026-07.md`](./reports/audit-2026-07.md) | Full-project audit (2026-07): goals-vs-implementation, test attestation, CI shape, IoT security posture, docs coverage, perf/memory practice — findings, dispositions, and prioritized recommendations. |

## Module reference

See [`reference/README.md`](./reference/README.md) for the one canonical table (one row per
workspace crate: layer, role, source directory, and rustdoc link) — folded from what used to be
two near-duplicate tables (this file's own, and `api-reference.md`'s).

A few historical per-crate notes remain under [`reference/modules/`](./reference/modules/) for
GitHub browsing; they are out of the book and unmaintained — rustdoc is the authoritative API
reference.

`bin-zephyr` was the Zephyr `west`-built MCU app; it has been retired by the Zephyr→Embassy
pivot (its platform crate and `bin/zephyr/` are gone). The current MCU path is
`crates/bin/mcu-platform` + `crates/bin/mcu-rp2350` — see [`reference/README.md`](./reference/README.md)
for the up-to-date crate roster and its stale-roster flag.

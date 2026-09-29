# 0002 — Runtime hardware auto-detect descoped

**Status:** Locked (2026-07-16)

## Context

The legacy AWS pipeline probes RS-485 DIP addresses at startup to auto-detect whether a battery or
an inverter is attached at each address, resolving device identity with offline caching so a
technician never has to hand-configure which address is which
(`docs/gap-analysis-meteor-parity.md`'s "Legacy AWS pipeline — not re-homed" section, "Runtime
hardware auto-detection"). A related, simpler capability — a standalone RS-485 DIP-address bus
scanner that just walks all 16 addresses to find live units, a useful field diagnostic tool — also
has no sentient-rs equivalent (same section, "Standalone RS-485 DIP-address bus scanner").
`crates/bin/rpi/src/scan.rs` exists as gap-closure scaffolding but is a stub, not a working
scanner.

sentient-rs's actual startup behavior today is simpler: `poll.dip_addresses` (or each
`[[bus]].poll.dip_addresses`) is a fixed, operator-configured list — there is no probing, caching,
or identity-resolution step of any kind.

## Decision

**Descope runtime hardware auto-detection.** Configured DIP addresses stand as the sole source of
truth for what this device polls. No startup bus-scan, no offline-cached identity resolution, no
auto-classification of "battery vs. inverter" at an address — this is a live concern, not a moot
one, since decision 0001 (2026-07-16) supersedes its earlier descope call and brings the RS-232
inverter leg in scope alongside the RS-485 battery leg; per-bus role (battery vs. inverter) is
still resolved from explicit `[[bus]]` config, never auto-detected.

## Consequences

- Commissioning a device requires a human (or a provisioning script) to know and set the correct
  DIP address(es) in config ahead of time — a misconfigured or forgotten DIP address is a silent
  "never polls that pack" failure mode, not a startup error, unless caught by preflight
  (`lib-preflight`'s `dip_addresses_present` check only verifies the *list* is non-empty, not that
  it matches real hardware).
- `crates/bin/rpi/src/scan.rs`'s stub is a placeholder for the field-diagnostic scanner
  specifically (the simpler of the two legacy capabilities) — filling it in is decoupled from this
  decision and remains a small, independently worthwhile follow-up even with auto-detect itself
  staying out of scope.
- No offline identity cache exists — a device's DIP-to-role mapping lives only in
  `sentient.toml`/`[[bus]]`, which is also what makes it auditable/reviewable (a config diff shows
  exactly what changed), unlike a runtime-learned cache.

## Revisit trigger

- Commissioning-at-scale pain becomes a real operational cost (e.g. many units per site, frequent
  DIP reconfiguration, or field techs mis-wiring addresses often enough that a scan-and-confirm
  step would measurably reduce truck rolls).
- Already triggered: decision 0001 (2026-07-16) brought the RS-232 inverter leg in scope this
  round (see `0001-rs232-inverter-leg-in-scope.md`, which supersedes its earlier descope call), so
  "which address is a battery vs. an inverter" is now a real per-deployment classification
  question — resolved by explicit `[[bus]]` config under this decision, not by runtime probing.

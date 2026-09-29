# Summary

[Introduction](introduction.md)

# Architecture

- [System architecture](architecture.md)
- [Wire serialization](architecture/wire-serialization.md)
- [Crate dependency graph](architecture/crate-graph.md)
- [Decision records (ADRs)](decisions/README.md)
  - [0001 — RS-232 inverter leg: in scope](decisions/0001-rs232-inverter-leg-in-scope.md)
  - [0001 — RS-232 inverter leg: descoped (superseded)](decisions/0001-rs232-inverter-leg-descoped.md)
  - [0002 — Runtime HW autodetect descoped](decisions/0002-runtime-hw-autodetect-descoped.md)
  - [0003 — TMS device role: STOR](decisions/0003-tms-device-role-stor.md)
  - [0004 — Multi-port in-process](decisions/0004-multi-port-in-process.md)
  - [0005 — RS-485 direction mechanism](decisions/0005-rs485-direction-mechanism.md)
  - [0006 — Meshtastic protobuf approach](decisions/0006-meshtastic-protobuf-approach.md)
  - [0007 — TPM/TEM naming](decisions/0007-tpm-tem-naming.md)
  - [0008 — Passive serial streaming descoped](decisions/0008-passive-serial-streaming-descoped.md)
  - [0009 — spec-app ports and adapters](decisions/0009-spec-app-ports-and-adapters.md)
  - [0010 — MCU runtime: Zephyr → Embassy](decisions/0010-mcu-runtime-embassy.md)
  - [0011 — MCU security boundary (RP2350)](decisions/0011-mcu-security-boundary.md)
  - [0012 — Delight features: scope and seams](decisions/0012-delight-features-scope-and-seams.md)
  - [0013 — Transport security: trust store behind a seam](decisions/0013-transport-security-trust-store.md)

# Process

- [V-Model alignment](process/v-model.md)

# Security

- [Security posture](security/README.md)
- [Security integration map](security/integration-map.md)
- [Signing & verification](security/signing-and-verification.md)
- [Rust hardening](security/rust-hardening.md)
- [wolfCrypt backend](security/wolfcrypt-backend.md)
- [TPM signing report (Scope E)](security/scope-e-tpm-signing-report.md)
- [ARM TrustZone deep dive](security/arm-trustzone-deep-dive.md)

# TMS Compliance

- [TMS compliance: field-by-field](compliance/README.md)
- [Attestation pipeline (OSCAL · policy · evidence)](compliance/attestation.md)
- [Type-catalogue coverage report](compliance/tms/tms-coverage.md)
- [TAA applicability carve-out](compliance/taa-applicability.md)

# Deployment

- [Build, test, run](deployment/usage.md)
- [On-device deployment: RPi flashing & install](deployment/README.md)
- [RS-485 wiring & transceiver prerequisites](deployment/rs485-wiring.md)
- [Yocto image & OS hardening](deployment/distro.md)
- [MCU firmware: RP2350 build & flash](deployment/mcu-firmware.md)
- [Release lifecycle & promotion](deployment/release-lifecycle.md)
- [MQTT visualization stack (Grafana)](deployment/grafana-viz.md)
- [Demo: disk logging](deployment/demo-disk-logging.md)

# Testing

- [Seplos simulator & e2e harness](testing/seplos-sim.md)
- [QEMU image testing](testing/qemu.md)
- [MCU DDS SPDP discovery — HIL](testing/mcu-dds-spdp-hil.md)
- [Delight-features MCU bench — wiring & HIL sign-off](testing/mcu-delight-features-hil.md)

# Performance

- [Pi hot-path benchmark](perf/pi-hotpath-benchmark.md)
- [RP2350 footprint & perf](perf/rp2350-footprint-perf.md)

# Reference

- [Chipset compatibility & portability](reference/chipset-compatibility.md)
- [Reference-design BoM & indicative cost](reference/reference-bom.md)
- [Rust API docs (rustdoc)](reference/README.md)

---

[Audit 2026-07](reports/audit-2026-07.md)
[Gap analysis: METEOR parity](reports/gap-analysis-meteor-parity.md)

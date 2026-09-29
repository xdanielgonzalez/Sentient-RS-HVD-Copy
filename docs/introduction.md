<p class="eyebrow">Sentient METEOR · Engineering Documentation</p>

# Introduction

> **Live site** — this book is served tailnet-only at
> [`sentient-docs.tail4e933e.ts.net`](http://sentient-docs.tail4e933e.ts.net) and republishes
> automatically on every merge to `main` (build: `docs.yml` → image → k3s rollout; hosting
> details: `.ci/k8s/docs/`). Local preview: `just docs-serve`.

**sentient-rs** is the production Rust telemetry adapter for the Sentient METEOR TPM/TEM: it
polls a Seplos BMS over RS-485, maps readings to TMS (MIL-STD-3071), and publishes them over
MQTT. It ships on a Raspberry Pi comms module (the paid deliverable, on a hardened Yocto Linux
image) with a Zephyr MCU forward path. The protocol/mapping core is platform-free and `no_std`
(it builds for `thumbv7em-none-eabihf`); platform I/O lives behind traits and feature flags.

## Runtime data flow

The workspace reads as a **runtime data-flow stack** — telemetry rises from the physical device
at the bottom, up through decode → model → mapping, and out the top via the MQTT interface +
network, with a **runtime** band that runs the app, provides the platform I/O, and secures it:

{{#include ../README.md:dataflow}}

## How this book is organized

The book is arranged in six parts. **Architecture** covers the system design, wire
serialization, the enforced crate dependency graph, and the decision records (ADRs) behind them.
**Deployment** walks build/test/run, Raspberry Pi flashing and install, the Yocto image and its
OS hardening, the Zephyr MCU build, and the release lifecycle with signing.
**TMS Compliance Attestation** documents the MIL-STD-3071 field-by-field mapping, the
OSCAL/policy attestation pipeline, and the generated type-catalogue coverage report.
**Security** collects the security posture, integration map, Rust hardening, the wolfCrypt
backend, and the TPM/TrustZone deep dives. **Testing** describes the Seplos simulator and e2e
harness plus the Renode and QEMU emulation rigs. **API Reference** links into the generated
rustdoc for every workspace crate.

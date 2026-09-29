# sentient-rs

Production Rust telemetry adapter for the Sentient METEOR TPM/TEM: poll a Seplos BMS over RS-485,
map to TMS (MIL-STD-3071), and publish over MQTT. Runs on a Raspberry Pi comms module (the paid
deliverable) with a Zephyr MCU forward path. The protocol/mapping core is platform-free and
`no_std` (builds for `thumbv7em-none-eabihf`); platform lives behind traits + feature flags.

## Architecture

The workspace reads as a **runtime data-flow stack** — telemetry rises from the physical device
at the bottom, up through decode → model → mapping, and out the top via the MQTT interface +
network; a **runtime** base runs the app, provides the platform I/O, and secures it. Bands group
into three zones (egress · data · runtime).

<!-- ANCHOR: dataflow -->
```mermaid
flowchart TB
    subgraph L_interface["interface · the MQTT broker the world subscribes to"]
        direction LR
        c_lib_mqtt["lib-mqtt"]
    end
    subgraph L_transport["transport · carry telemetry out over the network"]
        direction LR
        c_lib_net["lib-net"]
    end
    subgraph L_presentation["presentation · canonical → TMS wire form"]
        direction LR
        c_lib_mapping["lib-mapping"] ~~~ c_lib_tms["lib-tms"]
    end
    subgraph L_model["model · compact in-memory telemetry"]
        direction LR
        c_lib_canonical["lib-canonical"] ~~~ c_lib_store["lib-store"]
    end
    subgraph L_decode["decode · vendor wire → canonical"]
        direction LR
        c_lib_parse["lib-parse"]
    end
    subgraph L_serial["serial link · RS-485 / RS-232 bytes"]
        direction LR
        c_lib_io["lib-io"]
    end
    device["physical device · Seplos BMS · Voltronic inverter"]
    subgraph L_runtime["runtime · runs the app, provides the platform, secures it"]
        direction LR
        c_bin_rpi["bin-rpi"] ~~~ c_bin_rpi_platform["bin-rpi-platform"] ~~~ c_bin_zephyr_platform["bin-zephyr-platform"] ~~~ c_impl_app["impl-app"] ~~~ c_lib_security["lib-security"] ~~~ c_spec_app["spec-app"]
    end
    L_interface ~~~ L_transport ~~~ L_presentation ~~~ L_model ~~~ L_decode ~~~ L_serial ~~~ device ~~~ L_runtime
    device -.->|telemetry up| c_lib_io
    c_lib_mqtt -.->|out| out((subscribers · cloud))
    classDef crate fill:#ffffff,stroke:#b6bcc4,color:#1a1a1a;
    class c_lib_mqtt,c_lib_net,c_lib_mapping,c_lib_tms,c_lib_canonical,c_lib_store,c_lib_parse,c_lib_io,c_bin_rpi,c_bin_rpi_platform,c_bin_zephyr_platform,c_impl_app,c_lib_security,c_spec_app crate
    classDef hw fill:#f4f4f6,stroke:#b0b0b0,color:#333,stroke-dasharray:4 3;
    class device,out hw
    style L_interface fill:#d6e8e2,stroke:#8fb3a8,color:#1a1a1a
    style L_transport fill:#d6e8e2,stroke:#8fb3a8,color:#1a1a1a
    style L_presentation fill:#dde7f2,stroke:#9db0c8,color:#1a1a1a
    style L_model fill:#dde7f2,stroke:#9db0c8,color:#1a1a1a
    style L_decode fill:#dde7f2,stroke:#9db0c8,color:#1a1a1a
    style L_serial fill:#dde7f2,stroke:#9db0c8,color:#1a1a1a
    style L_runtime fill:#e6e3ee,stroke:#b0a9c2,color:#1a1a1a
```
<!-- ANCHOR_END: dataflow -->

This is the *runtime* view (how data moves). The complementary *compile-time* view — the acyclic
crate **dependency** layering that `cargo-deny` bans + the roster/orphan gate
([`.ci/crate-graph.toml`](.ci/crate-graph.toml)) enforce, including the `Platform` contract seam
(`spec-app`) that keeps the `impl-app` policy hardware-free
([ADR 0009](docs/decisions/0009-spec-app-ports-and-adapters.md)) — is the detailed dependency
graph in [`crates.svg`](docs/architecture/crates.svg). Regenerate both with `just depgraph`.

## Crates

| Layer | Crate | Responsibility |
|---|---|---|
| foundation | `lib-canonical` | Vendor-neutral canonical device model (`CanonicalReading`/`CanonicalFault` + inverter). Pure, `no_std`. |
| | `lib-tms` | Generated MIL-STD-3071 (TMS) wire model, emitted from the IDL. Pure, `no_std`. |
| | `lib-security` | Auth/cert/crypto behind a provider seam (FIPS + TrustZone toggles; wolfCrypt backend). |
| | `lib-store` | Bounded in-memory canonical telemetry store — the render-on-output memory model. Pure, `no_std`. |
| domain | `lib-parse` | Vendor protocol parsers (Seplos BMS, Voltronic inverter) → the canonical model. Pure, `no_std`. |
| | `lib-io` | `SerialTransport` async trait (generic, no `dyn`) + host tokio-serial / embassy MCU UART + generic poll driver + JSONL persist. |
| bridge | `lib-mapping` | The sole canonical↔TMS boundary: transform + generated wire encoders + the `Publisher` contract + legacy. |
| contract | `spec-app` | The app contract (ADR 0009): the `Platform` port + `RunConfig`/`Health` + preflight — the seam binaries inject through. |
| service | `lib-mqtt` | Embedded rumqttd MQTT server + `Publisher` over its in-process link (RPi side). |
| | `lib-net` | The device network layer: the owned MCU stack (channel/link/embassy) + host mesh-radio + location, feature-gated per target. |
| app | `impl-app` | The pure `run` loop (poll → decode → TMS → publish), generic over `spec_app::Platform`. Hardware-free. |
| platform | `bin-rpi-platform` | RPi platform adapter — `HostPlatform` (embedded broker + mesh/locate) implementing `spec_app::Platform`. |
| | `bin-zephyr-platform` | Zephyr MCU platform adapter — FFI + `ZephyrPlatform`, at `bin/zephyr/platform/`; a workspace member. |
| bin | `bin-rpi` | The Raspberry Pi Linux binary (the deliverable) — composition root that injects `bin-rpi-platform`. |
| | `bin-zephyr` | Zephyr `west` app (forward path): `rust_main` + the C shim; built via `west`, not `cargo`. |

Test crates (`test-kit`, `test-e2e`, `test-acceptance`) provide the shared sims/harness, live
end-to-end tests, and the Cucumber acceptance suite.

## Documentation

- [`docs/README.md`](docs/README.md) — full documentation index (guides + per-crate module reference).
- [`docs/deployment/README.md`](docs/deployment/README.md) — RPi install, wiring, config, mosquitto relay, systemd, runbook.
- [`docs/security/README.md`](docs/security/README.md) — security posture, boot integrity, TPM recommendations (Scope E).
- [`docs/security/signing-and-verification.md`](docs/security/signing-and-verification.md) — cosign keyless release provenance + X.509 CMS on-device binary verification, end to end.
- [`docs/compliance/README.md`](docs/compliance/README.md) — schema-shape compliance vs MIL-STD-3071.

## Build & test

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
# no_std core for the MCU forward path:
cargo build -p lib-parse -p lib-mapping -p lib-io -p impl-app \
  --no-default-features --target thumbv7em-none-eabihf
```

The Zephyr app builds via `west` (see `bin/zephyr/README.md`), never `cargo build` directly.

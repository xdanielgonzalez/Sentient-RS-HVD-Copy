# `bin-rpi` — the Raspberry Pi Linux binary (the deliverable)

The contract deliverable: the binary that runs on the Raspberry Pi comms module at each TPM/TEM. A
thin entry point over the shared `impl-app` loop.

## Role in the pipeline

`bin-rpi` is the wiring/entry layer — it holds no pipeline logic of its own. It:

1. Loads `Config` (TOML + `SENTIENT_`-prefixed env, via `figment`),
2. Builds the RPi platform via the factory (`bin_rpi_platform::HostPlatform::build`) —
   starting the embedded rumqttd broker (owned here, in `bin-rpi`, and handed in as a
   `BrokerPublisher` per bus — see `src/runtime.rs`) and opening the configured serial transport,
3. Runs the `spec_app::preflight::Preflight` checks (`platform.preflight().await`) and **refuses to
   start the poll loop if any check `Fail`s, except `serial_present`**
   (`report.ok_except("serial_present")`) — a missing/unopenable serial port is the one condition
   `HostPlatform::build` already degrades gracefully for (broker up, `HostSerial::Disconnected`,
   background `reconnect()` retries — bug #172); every other check (config validity, cloud creds,
   mosquitto bridge) still blocks startup. See `preflight_inputs()` in `src/main.rs` for how the
   config maps to `bin_rpi_platform::PreflightInputs`,
4. Signals systemd readiness (`bin_rpi_platform::notify_ready`),
5. Hands the platform to the shared poll loop (`impl_app::run`), racing it against `Ctrl-C` for
   clean shutdown (`tokio::select!`).

## `no_std` status

**std binary** (the RPi Linux deliverable). `#[tokio::main]`. Installs `env_logger` as the `log`
backend.

## Key items

| Item | What it is |
|---|---|
| `[[bin]] bin-rpi` | The binary (`src/main.rs`). |
| `config::Config` | The parsed TOML config (`src/config.rs`): `[device]`, `[serial]`, `[poll]`, `[broker]`, `[publish]` sections; maps into the io/broker constructor types (`serial_kind()`, `serial_params()`, `broker_config()`). |
| `config_path()` | Config resolution: `SENTIENT_CONFIG`, then `config/sentient.toml` / `sentient.toml`, else env-only. |
| `run_config()` | Builds the platform-neutral `RunConfig` (bounds DIP addresses to `MAX_DIPS`, truncates ids into bounded `heapless::String`s). |

The full config schema, install steps, systemd unit, and runbook live in
[`../deployment.md`](../../deployment/README.md); the build/run commands are in [`../usage.md`](../../deployment/usage.md).

## Dependencies and dependents

- **Depends on:** `impl-app` (feature `inverter`; the policy loop + `run_inverter`), `bin-rpi-
  platform` (the RPi `HostPlatform` adapter, ADR 0009), `spec-app` (the contract — `RunConfig`/
  `MAX_DIPS`/`preflight::Preflight`, named directly rather than via `impl-app`'s re-exports —
  see `runtime.rs`), `lib-io`, `lib-mapping`, `lib-parse`, `lib-mqtt`, `lib-net` (feature `host`),
  `tokio`, `rumqttc`, `log`, `env_logger`, `figment`, `serde`, `serde-json-core`, `anyhow`,
  `heapless`.
- **Depended on by:** none — it is the top-level binary. `just build-release <target>` /
  `build-auditable <target>` build it for the profiling profile (see [`../usage.md`](../../deployment/usage.md)).
  It is excluded from `just coverage` (`--exclude bin-rpi`, it is a thin shell).

## Testing

No in-crate tests (thin entry point); its behaviour is covered by the pipeline tests in
`impl-app` and `lib-mqtt`, and end-to-end by the deployment runbook checks in
[`../deployment.md`](../../deployment/README.md). Release binaries are signature-verified at service start via
`distro/deploy/verify-binary.sh` (`ExecStartPre` in the systemd unit).

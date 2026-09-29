# `impl-app` — the shared adapter application

One poll → decode → TMS map → publish loop, parameterized over a `Platform` factory. This is where
the whole pipeline is wired together; the two entry points (`bin-rpi`, `bin-zephyr`) are
thin shells that build a platform and call `run`.

## Role in the pipeline

`impl-app` owns the control loop and stitches the layers:

```
lib-io driver ─▶ lib-parse ─▶ lib-mapping transform ─▶ lib-mapping::publish_tms
        (read_telemetry / read_fault)                                       (Platform::publisher)
```

The pipeline body is **identical across targets** — only the `Platform` differs: the RPi
platform lives in the `bin-rpi-platform` crate (no module doc page yet); the former Zephyr MCU
platform lived with its entry point (`bin-zephyr-platform`, now archived — deleted in the
Zephyr→Embassy pivot: [`bin-zephyr-platform.md`](../../archive/reference-bin-zephyr-platform.md);
its successor `bin-mcu-platform` has no module page yet, a follow-up gap). `impl-app` itself
holds no platform impl — it is pure policy (`run`/`run_cycle`/`run_inverter` + the canonical
arena + `Backoff`) parameterized over the `Platform` port (ADR 0009, ports & adapters — see
[`../decisions/0009-spec-app-ports-and-adapters.md`](../../decisions/0009-spec-app-ports-and-adapters.md)).

## `no_std` status

Unconditionally `#![no_std]` + `#![forbid(unsafe_code)]` (ADR 0009 WS3) — every platform impl
(the RPi adapter's `tokio`/`lib-mqtt`/`anyhow`/`sd-notify` stack included) now lives outside this
crate, in `bin-rpi-platform`. Features:

| Feature | Effect |
|---|---|
| `inverter` | Enables the RS-232 inverter leg (`run_inverter`/`run_cycle_inverter`): `lib-parse/voltronic`. Purely a parser-linkage gate, no platform/std pull-in. |
| `default` | none — `bin-rpi` selects `inverter` (it spawns `run_inverter`); the MCU build never enables it. |

## Key public types

| Item | What it is |
|---|---|
| `run` | The forever poll loop (`async fn run<P: Platform>(platform, cfg) -> !`): owns cadence, exponential backoff, the watchdog hook, and the periodic health summary. |
| `run_cycle` | One poll cycle (telemetry per DIP, faults on the `fault_every` cadence, publish each mapped payload). Split out of `run` so it is testable in isolation; returns a `CycleOutcome`. |
| `Platform` | The per-target factory trait: `transport()`, `publisher()`, `now()`, `sleep()`, `on_cycle()`, `reconnect()`. Associated `Transport: SerialTransport` + `Pub: Publisher`. Now defined in `spec-app` (ADR 0009); re-exported here for path stability — no `ActivePlatform` alias, each entry point names its own concrete type (`bin_rpi_platform::HostPlatform` / `bin_zephyr_platform::ZephyrPlatform`). |
| `RunConfig` | Platform-neutral, bounded run params (device id, prefix, DIP addresses, interval, `fault_every`). `MAX_DIPS = 16`. Defined in `spec-app`, re-exported here. |
| `Health` | Saturating `u32` health counters (telemetry/fault/publish ok/err, consecutive failed cycles); `healthy()` is the signal the host gates the systemd watchdog on. Defined in `spec-app`, re-exported here. |
| `Backoff` | Exponential dead-cycle backoff, 1 s → 30 s cap, saturating. |
| `preflight` | The startup preflight-check subsystem (`Check`/`Outcome`/`Preflight`/`Report`/`run_checks!`). Defined in `spec-app::preflight`, re-exported here. |

Tuning constants: `RECONNECT_AFTER_CYCLES = 3`, `UNHEALTHY_AFTER_CYCLES = 3`,
`SUMMARY_EVERY_CYCLES = 30`. The health log line and failure-mode behaviour are documented
operationally in [`../deployment.md`](../../deployment/README.md) §7–§8.

## Dependencies and dependents

- **Depends on:** `spec-app` (the `Platform`/`RunConfig`/`Health`/`preflight` contract, ADR 0009),
  `lib-parse` (seplos; + `voltronic` under the `inverter` feature), `lib-mapping`, `lib-io`, `log`,
  `heapless`. Unconditionally `no_std` — no `lib-mqtt`/`tokio`/`anyhow`/`sd-notify` (those moved to
  `bin-rpi-platform`, the RPi adapter).
- **Depended on by:** `bin-rpi` (the composition root — calls `run`/`run_inverter` directly,
  injecting a `bin_rpi_platform::HostPlatform` built by the adapter crate; `bin-rpi-platform`
  itself takes **zero** `impl-app` dependency, the point of the ADR 0009 inversion),
  `bin-zephyr-platform` (the MCU platform crate — it consumes `run`/`arena`/`Platform` from here,
  the one surviving detail→policy edge). Also a dev-dependency of `test-e2e`/`test-acceptance`.

## Testing

- `tests/run_cycle.rs` — drives `run_cycle` over the `lib-io` `mock` transport with a small
  executor, asserting telemetry/fault/publish accounting in isolation.

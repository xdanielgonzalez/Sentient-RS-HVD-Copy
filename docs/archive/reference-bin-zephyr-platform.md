# `bin-zephyr-platform` — the Zephyr MCU platform, colocated with its entry point

The Zephyr implementations of the platform seams — async UART transport, publish sinks, the
device-hosted broker pump, the monitor logger, and the `ZephyrPlatform` factory — bound to the
Zephyr C shim (`bin/zephyr/src/zephyr_shim.c`) via extern-C FFI. Lives at `bin/zephyr/platform/`
because the entry point owns its platform, but unlike the `west`-built `bin-zephyr` app it **is a
cargo workspace member**: the crate uses no `zephyr` crate and no Zephyr tree, so the normal host
clippy/test gates compile its entire surface (`[lib] test = false` keeps host test binaries from
ever linking the `sz_*` symbols, which only exist in the west link).

## Role in the pipeline

`bin-zephyr` (`rust_main`) builds [`ZephyrPlatform`] from this crate and hands it to
`impl_app::run` — the same shape `bin-rpi` uses with `bin_rpi_platform::HostPlatform`. The C shim
supplies the `sz_*` symbols; this crate supplies everything Rust above them.

```
bin-zephyr (west app: rust_main + C shim)
        └─▶ bin-zephyr-platform (this crate: FFI adapters + factory)
                    └─▶ impl-app run loop ─▶ lib-parse ─▶ lib-mapping ─▶ Publisher
```

## Key public items

| Item | What it is |
|---|---|
| `ZephyrPlatform` | The MCU `impl_app::Platform` impl (UART transport, publisher, clock, watchdog feed). Implements `lib_preflight::Preflight` (`SerialPresent`, `ConfigValid`, `UartReady`, `WatchdogReady`, `EntropySourceReady`). |
| `ZephyrPublisher` | Runtime-selected sink: device-hosted-broker arena path or remote MQTT client (`ZephyrMqttPublisher`), absorbed behind the shared `Publisher` trait. |
| `rust_broker_main` | `extern "C"` broker-pump entry (spawned by a `K_THREAD_DEFINE` in the shim): drives `lib_link::mqtt_server::BrokerCore` over `lib_link::broker_transport`, rendering TMS lazily from the canonical arena. |
| `MonitorLogger` / `monitor_producer` / `monitor_consumer` | The `log` facade tee: console printk + a bounded SPSC queue the pump retained-publishes on the monitor topic. |
| `block_on` | Minimal single-poll executor for the entry point (the poll futures wrap blocking C calls and never truly suspend). |
| `SpinArena` / `ARENA_DEPTH` | The spinlock-guarded static `CanonicalArena` handoff between the poll thread and the pump thread (store-canonical, render-on-output). |

## `no_std` / safety status

`#![no_std]`, no alloc. Unsafe is intrinsic here (it is the FFI boundary) — every unsafe
operation carries a `// SAFETY:` comment stating its invariant; the workspace clippy
`unwrap`/`expect`/`panic` denies apply via `[lints] workspace = true`. This is the **only**
first-party crate that permits `unsafe` (`impl-app` itself is `#![forbid(unsafe_code)]`).

## Dependencies and dependents

- **Depends on:** `impl-app` (run loop, arena, `Platform`), `lib-parse` (canonical types),
  `lib-mapping` (render), `lib-io` (`SerialTransport`), `lib-preflight`, `lib-link`
  (`default-features = false` — `BrokerCore` + the broker transport seam), `heapless`, `log`.
- **Depended on by:** `bin-zephyr` (the west app — the only consumer; this crate exists to serve
  that entry point).

## Testing

No in-crate tests (`[lib] test = false`): host binaries cannot link `sz_*`. The behavior is
attested from the outside — the aliasing pattern by `lib-link`'s Miri oracle
(`broker_pump_aliasing_miri.rs`, `just miri`), the concurrency shapes by `impl-app`'s host ports
(`monitor_queue_repro.rs`, `arena_emit.rs`), and the whole firmware by the QEMU/Renode lanes once
#191's re-enable lands. Compile gates: workspace clippy, `just lint-zephyr` (focused), and
`just build-no-std` (thumbv7em).

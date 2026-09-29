# 0004 — Multi-port serial = in-process, not one process per bus

**Status:** Locked (2026-07-16)

## Context

The legacy Python prototype runs an `InterfaceManager` that drives N concurrent serial interfaces
(mixed RS-232/RS-485/Seplos) fanning into one queue. Before this workstream, sentient-rs hard-coded
exactly one port per process (`bin-rpi/src/config/types/serial_config.rs` was a single struct;
`HostPlatform` owned exactly one transport) — flagged as a `high`-severity, decision-needed gap in
`docs/gap-analysis-meteor-parity.md`'s "Multi-port serial (InterfaceManager)" finding, since METEOR
units are modular and multi-module: a real deployment may have more than one Seplos pack bus (or,
before decision 0001 descoped it, an inverter leg on a second port) that all need to report through
the same device identity.

Two shapes were live candidates for closing this gap:

1. **One OS process per configured bus** — each bus gets its own `bin-rpi` instance, its own
   embedded MQTT broker, its own systemd unit. Simple to reason about per-bus, but N buses means N
   broker instances, N mosquitto bridge connections, and N independent watchdog/health surfaces to
   monitor per device — multiplying ops surface with every added bus.
2. **One process, N poll-loop tasks, one shared broker** — a single `bin-rpi` supervises every
   configured `[[bus]]` as its own `impl_app::run` tokio task, sharing one embedded broker (each
   bus mints its own cheap `BrokerPublisher` off it) and one aggregated systemd watchdog feed.

## Decision

**In-process, one supervisor process per device, N poll-loop tasks per configured bus.**
Implemented in `crates/bin/rpi/src/runtime.rs::supervise`: `Config::buses()` normalizes either the
legacy single `[serial]` section or a `[[bus]]` array-of-tables into a canonical
`Vec<BusConfig>` (mutually exclusive — configuring both is a hard startup error, not a silent
pick); one embedded `LocalBroker` is started once, and each bus's `HostPlatform` is built with its
own publisher handle off that shared broker (cheap — shares the broker's `LinkTx` via `Arc`); every
bus's poll loop, plus the shared secondary-publish background tasks (disk writer, mesh radio,
location refresher, heartbeat, reply listener), joins one `JoinSet` supervised by a single
graceful-shutdown sequence.

## Consequences

- **A `WatchdogAgg` aggregation layer became necessary** (`bin_rpi_platform::WatchdogAgg` as of
  ADR 0009's `impl_app::host` → `bin-rpi-platform` extraction): the
  systemd watchdog can only feed `WATCHDOG=1` once per process, so every bus and every secondary
  background task registers its own `CyclePulse` slot, and the aggregator withholds the feed unless
  *every* registered slot has pulsed recently — a single wedged bus (or a wedged disk writer) now
  correctly starves the watchdog for the whole process, not just silently degrading one bus while
  the others (and the watchdog) look healthy.
- **Fan-out taps are per-bus, background tasks are shared:** each bus gets its own `SinkSet` (so a
  disk-logged record correctly carries `bus: "pack-a"` vs. `"pack-b"`), but the underlying
  `lib_persist::run_writer`/`lib_mesh::run_radio` tasks are constructed once and shared — avoiding
  N duplicate writer/radio tasks for what should be one on-disk log / one radio.
- **Shutdown is bounded and ordered, not per-bus ad hoc:** SIGTERM/Ctrl-C/an unexpected port-task
  exit all funnel through one `tokio::select!`, `notify_stopping()`, abort every port task, signal
  the shared `watch::channel` shutdown broadcast, then one 5 s drain timeout across both `JoinSet`s
  — see `crates/test-e2e/tests/sigterm_drain_e2e.rs` for the black-box proof this actually exits 0
  and flushes/fsyncs a real disk-logged JSONL file within that window.
- **Ops surface stays flat as buses are added:** one systemd unit, one broker, one mosquitto bridge
  connection, one watchdog — adding a second `[[bus]]` entry to config is the only change needed to
  add a second poll loop; no new process/unit/port to provision.
- **A crash in one bus's poll task brings the whole process down** (an unexpected `port_tasks.
  join_next()` exit triggers the same shutdown sequence as SIGTERM) — the flip side of sharing one
  process: buses are not failure-isolated from each other at the process level, only isolated at
  the tap/publish level. A misbehaving bus can take a healthy sibling bus down with it.

## Revisit trigger

- Per-bus failure isolation becomes a hard requirement (a single wedged/crashing bus must never
  affect a sibling bus's uptime) — that would push back toward option 1 (one process per bus) or a
  supervisor-of-processes redesign, neither of which is free: both reintroduce the N-broker/N-unit
  ops-surface cost this decision explicitly avoided.
- The number of buses per device grows large enough that the shared-broker/shared-watchdog model's
  aggregation (one wedged slot starves the whole feed) becomes operationally noisy rather than
  correctly conservative.

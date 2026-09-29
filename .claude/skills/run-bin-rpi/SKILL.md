---
name: run-bin-rpi
description: Build, run, and drive bin-rpi (the Raspberry Pi Seplos BMS -> TMS -> MQTT adapter). Use when asked to run bin-rpi, start the adapter, smoke-test the poll/publish pipeline, or confirm a change to bin-rpi/impl-app/lib-io/lib-mqtt/lib-mapping/lib-parse actually publishes telemetry over MQTT — not just that it compiles or its unit tests pass.
---

`bin-rpi` (`crates/bin/rpi`) is a long-running tokio binary: it polls a Seplos BMS over a serial
port, decodes + maps to TMS, and publishes onto an embedded MQTT broker (rumqttd). It has no GUI
and needs real hardware (a Seplos pack over RS-485) to run as-shipped — this skill instead points
it at a **simulated Seplos device on a pseudo-terminal** so it can be driven headless, in this
container, with no hardware. Drive it via
`.claude/skills/run-bin-rpi/driver.py` (Python 3, stdlib + `paho-mqtt`) — it owns the simulator, the
generated config, and an MQTT subscriber to observe what gets published. All paths below are
relative to the repo root (`/mnt/d/hvd/sentient-rs`).

## Prerequisites

Already present in this container: `cargo`/`rustc`, `python3`, and `paho-mqtt` (`pip list | grep
paho-mqtt` -> `paho-mqtt 2.1.0`, installed at the user level). Nothing else to install — no
`socat`/`mosquitto-clients` needed; the driver opens its own pty via `os.openpty()` and subscribes
via `paho-mqtt` directly.

## Build

```bash
cargo build -p bin-rpi
```

Takes ~1 minute cold (`target/debug/bin-rpi`). The driver also builds automatically on first `up`/
`smoke` if the binary is missing, or if you pass `--build`.

## Run (agent path)

The driver has four subcommands: `up` (launch, foreground/blocking), `sub` (subscribe and print),
`down` (stop a backgrounded `up`), and `smoke` (bounded one-shot: up + sub + down, exit code tells
you pass/fail). **`smoke` is the fastest way to confirm a change didn't break the poll/publish
pipeline:**

```bash
rm -rf /tmp/bin-rpi-driver
python3 .claude/skills/run-bin-rpi/driver.py smoke --seconds 6 --port 18830
echo "exit=$?"
```

This builds (if needed), opens a pty, starts a responder thread that answers every Seplos command
frame with a real captured 14-cell telemetry reply, writes a `sentient.toml` pointing `bin-rpi` at
the pty's slave path, launches `bin-rpi`, waits for `embedded MQTT server up on 127.0.0.1:<port>` in
its log, subscribes to `tms/#`, prints every message it receives for `--seconds`, then tears
everything down. **Exit 0 with `tms/#` messages printed (topics like
`tms/METEOR-DRIVER/dc_measurement_update`, `.../power_hardware_update`, `.../storage_update`,
`.../heartbeat`) means the whole pipeline — serial poll, Seplos decode, TMS mapping, MQTT publish —
is working end to end.** Exit 1 means no messages arrived; check `/tmp/bin-rpi-driver/bin-rpi.log`.

For interactive use (launch once, poke it multiple times, then stop), use the background-launch +
verify + stop pattern instead:

```bash
# launch in the background (long-running; the process itself blocks, so background it)
nohup python3 .claude/skills/run-bin-rpi/driver.py up --port 18830 > /tmp/bin-rpi-up.log 2>&1 &
disown
sleep 3   # `up` waits internally for readiness before returning control to bin-rpi's poll loop,
          # but the driver process itself is now backgrounded — give it a moment to get there

# verify: subscribe and print whatever comes in over N seconds
python3 .claude/skills/run-bin-rpi/driver.py sub --port 18830 --seconds 5

# stop cleanly (reads /tmp/bin-rpi-driver/state.json for the PIDs, SIGTERMs both, SIGKILLs after 0.5s)
python3 .claude/skills/run-bin-rpi/driver.py down
```

State (workdir, driver PID, `bin-rpi` PID, broker port, pty slave path, config path) lives at
`/tmp/bin-rpi-driver/state.json` (override the workdir with `--workdir`). `bin-rpi`'s own stdout+
stderr (its `env_logger` output — `RUST_LOG=info` by default) land in
`/tmp/bin-rpi-driver/bin-rpi.log`.

| command | what it does |
|---|---|
| `up [--port N] [--interval-ms N] [--seconds N] [--build] [--workdir D]` | launch simulator + bin-rpi; blocks (foreground) until `--seconds` elapses or SIGTERM/SIGINT (0 = run until signaled) |
| `sub [--port N] [--seconds N] [--topic T] [--require]` | connect to the broker, print every message on `--topic` (default `tms/#`) for `--seconds`; `--require` exits 1 if nothing arrived |
| `down [--workdir D]` | SIGTERM (then SIGKILL after 0.5s) the driver + bin-rpi PIDs from `state.json` |
| `smoke [--port N] [--interval-ms N] [--seconds N] [--build] [--workdir D]` | one-shot up+sub+down; exits 0 iff `tms/#` messages were received |

Default broker port is `18830` (loopback-only; harmless to collide with nothing since it's not the
production port `1883`). Default poll interval is 500ms (vs. production's 2000ms) so a short
`--seconds` window still sees a few cycles.

## Run (human path)

Same as production, just against a real Seplos pack on a real serial port instead of the driver's
pty simulator — see `docs/deployment.md` for wiring + config. Not meaningfully different from the
agent path above except there's no simulator: `SENTIENT_CONFIG=config/sentient.toml cargo run -p
bin-rpi`, Ctrl-C to stop.

## Test

```bash
cargo test --workspace
```

The real pty-based integration coverage this skill's simulator mirrors lives in
`crates/test-e2e/tests/host_pty.rs` and `seplos_at_rate_e2e.rs` (Rust, `nix`-based pty, no Python) —
useful reference if the driver's simulator ever needs to grow additional response behaviors
(fault injection, silence/timeout, corrupt frames — see `test_kit::seplos::SeplosSim`/`Behavior`
for the richer runtime-defined-battery-state version of this same idea, used by
`crates/test-acceptance`'s cucumber suite).

## Gotchas

- **A freshly-opened pty is in cooked/echoed mode by default** — `os.openpty()` gives you a slave
  whose termios still has `ICANON`/`ECHO` set. Writing to the master before raw mode is configured
  causes canonical line-buffering and echo-back, and a naive responder can end up replying to its
  own echoed reply in a loop. The driver calls `tty.setraw(slave_fd)` on the slave fd right after
  opening the pair, and — critically — **keeps `slave_fd` open** for the life of the process
  instead of closing it immediately. Closing every reference to the slave right after `setraw()`
  put the device into a state where `bin-rpi`'s own `HostSerial::open` (tokio-serial) couldn't
  reconfigure it — the real symptom wasn't an echo loop, it was **zero bytes ever reaching the pty
  in either direction**, timing out silently every poll cycle. If you're modifying the simulator,
  keep both `master_fd` and `slave_fd` open for the whole run; only close both in the `up` cleanup
  path.
- **`kind = "rs232"`, not `"rs485"`, in the generated config** — a pty has no RTS modem line, so
  RS-485 half-duplex direction toggling has nothing to act on. RS-232 is full-duplex and
  `set_direction` is a documented no-op for it — this is the same choice
  `crates/test-e2e/tests/host_pty.rs` makes, for the same reason.
- **The simulator answers every command with the same canned frame** regardless of DIP address or
  command type (telemetry vs. fault) — fine because the default `sentient.toml` only polls DIP 0
  and never requests fault flags (`fault_every = 0`). If you need multi-DIP or fault-command
  coverage, the simulator needs to inspect the command bytes instead of always replying with
  `LIVE_FRAME` (see `test_kit::seplos::SeplosSim::respond` for how the Rust-side fixture does this
  properly).
- **A stale `bin-rpi` left listening on the target port produces a false-positive smoke test,
  not an error.** `bin-rpi`'s embedded rumqttd doesn't fail loudly if its own bind loses a race —
  if something else already owns the port, the new `bin-rpi` process still logs "embedded MQTT
  server up" and its internal MQTT clients (heartbeat, publisher) happily connect as clients of
  the *stray* listener instead. You'd see `smoke` pass with plausible-looking messages that are
  actually coming from an old process, not the one you just launched (a telltale sign: two
  interleaved heartbeat `sequence_number` streams, one much higher than the other). `cmd_up` now
  refuses to start if `--port` is already accepting connections — if you hit the refusal, find and
  kill the stray process (`ps aux | grep bin-rpi`) rather than assuming it's safe to proceed.
- **`paho-mqtt`'s MQTT client needs an explicit `client_id`** — rumqttd (the embedded broker) sends
  a CONNACK error (`InvalidClientId`) for a client presenting an empty ID, which is `paho-mqtt`'s
  default unless one is passed. The driver's `sub` command passes `client_id="driver-sub"`.

## Troubleshooting

- **`smoke`/`up` hangs then exits 1 with "bin-rpi did not report ready"**: check
  `/tmp/bin-rpi-driver/bin-rpi.log` — most likely the broker port is already in use (pick a
  different `--port`) or the binary is stale/missing (`cargo build -p bin-rpi` first, or pass
  `--build`).
- **`sub` reports `received 0 message(s)` but `bin-rpi.log` shows successful telemetry publishes**:
  the subscribe likely started after the messages were already published, or connected to the
  wrong port — double check `--port` matches what `up`/`smoke` used (see `state.json`).
- **`down` says "no state file"**: nothing is running under that `--workdir`, or it already exited;
  check `ps aux | grep bin-rpi` for stragglers and `kill` directly if needed.

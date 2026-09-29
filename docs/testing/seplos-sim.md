<p class="eyebrow">Testing · Simulator & E2E Harness</p>

# Seplos BMS simulator — operator + developer guide

`SeplosSim` is the pure, transport-agnostic Seplos v2 BMS device responder that every test lane in
this repo — unit tests, the cucumber spec-alignment suite, the pty-based `test-e2e` crate, and the
QEMU/Yocto UAT harnesses — drives instead of real hardware. This doc covers the library API, the
`seplos-tcp-sim` binary that puts it on a TCP wire, the cucumber harness that drives it from
Gherkin, and how the QEMU/Yocto e2e scripts wire it in. For the Seplos wire protocol itself see
[`wire-serialization.md`](../architecture/wire-serialization.md); for how a `CanonicalReading`/`CanonicalFault`
maps to TMS see [`tms-compliance.md`](../compliance/README.md).

## 1. What it is

`SeplosSim` (`crates/test-kit/src/seplos/`) is a request-frame-bytes-in, response-frame-bytes-out
responder: `SeplosSim::respond(&mut self, request: &[u8]) -> SimReply` is a pure function of
`(BatteryState, Behavior, request bytes)`. It has **no I/O of its own** — no pty, no sockets, no
threads — so the exact same core answers a Seplos poll whether it's driven in-process by a unit
test, wrapped in a host pty by `test-e2e`, or put on a TCP socket by the `seplos-tcp-sim` binary for
the QEMU/Renode and Yocto lanes. It reuses `lib_parse::seplos::checksum` for all frame/LENGTH-LCHKSUM
math — the single authoritative implementation shared with the real decoder — so the sim and the
decoder can never silently drift apart; only ASCII-hex formatting/parsing is local to the module.

It was formerly the standalone `seplos-sim` crate, merged into `test-kit` in #99 so all test-support
lives in one dev-only crate (`publish = false`, never linked into a shipping binary or the `no_std`
MCU build).

### Fidelity status (tracked in [#226](https://github.com/High-Velocity-Decisions/sentient-rs/issues/226))

| Command | Status |
|---|---|
| `0x42` telemetry | **Wire-layout and length fidelity** confirmed against a live-hardware capture (`crates/lib-parse/tests/seplos_vectors.rs`'s `LIVE_FRAME`, a real 14-cell/6-temp Seplos 48150-1101-10E frame): the sim's encoded reply is the same 160-byte length and every field (cell mV, raw temps, current, total_voltage, capacities, soc/soh, cycles, port_voltage) lands at the live capture's byte offsets. The 4 trailing reserved `u16` words the live capture carries (`fff8 0000 0079 004d`) are **zeroed** by the sim rather than reproduced — `decode_reading` never reads past `port_voltage`, their real-world meaning isn't documented anywhere reachable, and no consumer compares raw capture bytes (see `impl_seplos_sim_encode.rs`'s doc comment). Separately, the sim's own `sample14` encoded output is pinned **byte-exact as a regression fixture** across refactors (`test-kit`'s `sample14_encoded_frame_is_byte_identical_after_the_spec_refactor` test, and the cucumber suite's `SAMPLE14_TELEMETRY_FRAME` constant) — this guards against silent encoder/spec-grammar drift, it is not a claim that those bytes equal `LIVE_FRAME` (which encodes a different battery reading: `sample_14_cell()`'s `total_voltage_cv` is *derived* from its cells, 5329, not the live capture's captured 5452). |
| `0x44` fault | Layout is an **empirical** `(byte, bit)` map (`crates/lib-parse/src/seplos/fault.rs`'s own doc comment: "observed on live hardware in the prototype, NOT confirmed against the Seplos spec") — a real 0x44 capture to verify it against is one of #226's open items. |
| `0xA4` serial number | Encoded per the documented protocol shape (leading `<` + ASCII serial), not yet cross-checked against a live capture. |

Other #226 follow-ons tracked alongside the 0x44 capture: RTN codes `01`/`05`, a "drift" behavior
mode, and wire-rate UAT (see [`audit-2026-07.md`](../reports/audit-2026-07.md)'s S13 item in its Appendix).

### Where it runs

- **In-process** — `test-kit`'s own unit tests (`crates/test-kit/src/seplos/mod.rs`'s `#[cfg(test)]`
  module) and the `test-acceptance` cucumber suite (`crates/test-acceptance/tests/steps/simulation.rs`)
  call `SeplosSim::respond` as a plain function, no sockets/ptys/threads.
- **Host pty e2e** — `crates/test-e2e/tests/seplos_at_rate_e2e.rs` and `lib-io`'s `tests/host_pty.rs`
  wrap `SeplosSim` in a real pty pair and drive it through the real `tokio-serial` transport.
- **TCP, for QEMU/Yocto lanes** — the `seplos-tcp-sim` binary (§3 below) puts `SeplosSim` on a TCP
  socket so an emulated UART (Renode/QEMU) or a `runqemu`-booted Yocto guest can poll it like a real
  RS-485 device.

## 2. Library API (`test_kit::seplos`)

Also re-exported at the crate root (`test_kit::{BatteryState, Behavior, SeplosSim, SimReply, ...}`).

### `BatteryState`

The source of truth `SeplosSim` encodes from. Field shapes/units mirror
`lib_parse::canonical::CanonicalReading`/`CanonicalFault` exactly:

| Field | Unit |
|---|---|
| `cell_voltages_mv: Vec<u16, MAX_CELLS>` | mV per cell |
| `temperatures_raw: Vec<u16, MAX_TEMPS>` | raw 0.1 K |
| `current_ca: i16` | 10 mA, signed (negative = discharge) |
| `total_voltage_cv: u16` | 10 mV / centivolts |
| `remaining_capacity_cah` / `typical_capacity_cah` / `rated_capacity_cah: u16` | 10 mAh |
| `soc_dpct` / `soh_dpct: u16` | 0.1 % |
| `cycles: u16` | charge-cycle count |
| `port_voltage_cv: u16` | 10 mV / centivolts |
| `faults: FaultFlags` | one bool per fault/warning bit |
| `serial: String<MAX_SERIAL>` | ASCII serial number |

Bounds: `MAX_CELLS = 16`, `MAX_TEMPS = 8`, `MAX_SERIAL = 32`, `MAX_FRAME = 256` (max on-wire response
length).

`BatteryState::sample_14_cell()` is the deterministic 14-cell/6-temp fixture used as the neutral
baseline everywhere: cells `3800..3813` mV, temps `2981,2983,2985,2987,2989,2991` (raw 0.1 K, i.e.
25.0–25.2 °C), `current_ca = -164`, `soc_dpct = 775`, `soh_dpct = 1000`, `cycles = 2`,
`remaining_capacity_cah = 1395`, `typical_capacity_cah`/`rated_capacity_cah = 1800`,
`port_voltage_cv = 5453`, no faults, serial `"SP-SIM-0001"`. `total_voltage_cv` is *derived*
(`sum(cell mV)/10`), never hardcoded, so the fixture is always internally consistent.

### `BatteryStateSpec` + the runtime spec grammar

`BatteryStateSpec` is a fluent builder (`.cells(&[u16])`, `.current_ca(i16)`, `.total_voltage_cv(u16)`,
… `.build() -> Result<BatteryState, String>`) that overlays individual field overrides atop the
`sample_14_cell()` baseline — every unset field inherits the baseline rather than defaulting to
zero, so a spec that only overrides one field still yields a fully plausible pack.

`parse_battery_state_spec(spec: &str) -> Result<BatteryState, String>` parses the textual,
whitespace-separated `key=value` grammar built on top of that same builder — the **one shared
parser** driving the `seplos-tcp-sim` `state` stdin verb (§3), the cucumber harness (§4), and Robot
Framework keywords, so all three can never drift apart. Example:

```text
cells=3800,3805,3810 temps=2981,2983 current=-164 soc=775 soh=1000 cycles=2 remaining=1750 typical=1800 rated=1800 port=5453
```

| Key | Unit | Bounds | `sample14` default |
|---|---|---|---|
| `cells` | comma-separated per-cell mV | 1..=`MAX_CELLS` (16) entries | 14 cells, `3800..3813` |
| `temps` | comma-separated raw 0.1 K | 1..=`MAX_TEMPS` (8) entries | 6 sensors, `2981,2983,2985,2987,2989,2991` |
| `current` | 10 mA, signed (negative = discharge) | fits `i16` | `-164` |
| `soc` / `soh` | 0.1 % | fits `u16` | `775` / `1000` |
| `cycles` | charge-cycle count | fits `u16` | `2` |
| `remaining` / `typical` / `rated` | 10 mAh | fits `u16` | `1395` / `1800` / `1800` |
| `port` | 10 mV / centivolts | fits `u16` | `5453` |
| `total` | 10 mV / centivolts (**OVERRIDE** — normally derived as `sum(cells)/10`) | fits `u16` | derived |

Every key is optional. `faults` and the serial number are **not** part of this grammar — faults are
set separately (the `faults` stdin verb / cucumber step; the serial number has no override seam
today). Fail-closed: an unrecognized key, a non-`key=value` token, a non-numeric value, or a
`cells`/`temps` count of `0` or over its max returns `Err(String)` — never a panic.

### `Behavior` — the fault-injection taxonomy

Set via `SeplosSim::set_behavior(Behavior)`; persists across calls until changed again.

| Variant | Effect |
|---|---|
| `Behavior::Normal` (default) | Byte-exact, checksum-valid reply synthesized from the current `BatteryState`. |
| `Behavior::CorruptChecksum` | A well-formed frame with one INFO byte flipped **after** assembly — checksum no longer matches. |
| `Behavior::WrongAddress` | A well-formed, checksum-**valid** frame addressed to a *different* DIP than the one polled (a real device answering for someone else, not a corrupted frame). |
| `Behavior::Truncated(usize)` | Only the first `n` bytes of the well-formed frame — never reaches the `\r` delimiter. |
| `Behavior::Silence` | No reply at all (a non-responding bus). |
| `Behavior::Garbage` | Non-frame bytes terminated by `\r` (garbage that still satisfies a delimiter read). |
| `Behavior::RtnError(u8)` | A well-formed, checksum-valid frame carrying the given non-`0x00` RTN code and empty INFO — how a real device reports a protocol-level error (`02H` CHKSUM, `03H` LCHKSUM, `04H` invalid CID2, …) rather than staying silent. |

### Auto-NAK (request-side classification)

`SeplosSim::set_auto_nak(bool)` — **off by default** (so every pre-existing "malformed request →
Silence" test keeps passing unchanged). When on, a request that fails the normal parse for one of
three protocol-recognizable reasons gets a protocol-correct RTN error frame instead of silence:

| Request defect | RTN code returned |
|---|---|
| Bad frame CHKSUM | `0x02` |
| Bad LENGTH LCHKSUM | `0x03` |
| Unrecognized CID2 | `0x04` |

Anything else (bad SOI, a truncated header, an unmodeled LENID shape) isn't reliably addressable
and still falls back to silence. **Auto-NAK is a Rust-API / cucumber-suite feature only** — there is
no stdin control verb or CLI flag for it today (see §3's control-line table).

### Request shapes accepted

`SeplosSim::respond` accepts both request shapes seen in practice: the repo's own
`lib_parse::seplos::command::build_command` form (`LENID=001`, a single hex-char INFO holding the
DIP nibble) and the documented protocol V2.0 / a real device's TX form (`LENID=002`, a 2-hex-char
INFO byte holding the command-group/pack selector, e.g. syssi's `esphome-seplos-bms` capture
`~20004642E00200FD37`) — the command-group byte is echoed back into the `0x42` reply.

## 3. The `seplos-tcp-sim` binary

`crates/test-kit/src/bin/seplos_tcp_sim/` — a std host binary (`cargo build -p test-kit --features
tcp-sim --bin seplos-tcp-sim`) that puts `SeplosSim` on the wire, reusing
`SeplosSim`/`BatteryState`/`Behavior` verbatim.

### CLI flags (verified against `impl_cli.rs`)

Exactly one of `--connect`, `--listen`, `--dump-fixtures` is required.

| Flag | Effect |
|---|---|
| `--connect HOST:PORT` | Dial out as a TCP **client** — the primary mode: Renode's `CreateServerSocketTerminal` and QEMU's `-serial tcp:...,server,nowait` make the emulator the server, so the sim connects in. |
| `--listen PORT` | Bind + accept as a TCP **server** — for local testing without an emulator. |
| `--dump-fixtures DIR` | No networking: run the offline `SeplosSim → lib_parse → lib_mapping → JSON` pipeline and write three golden TMS fixtures (`dc_measurement_update.golden.json`, `power_hardware_update.golden.json`, `active_diagnostic_state.golden.json`) into `DIR`, via the real `render_tms`/`render_fault` wire serializer — never hand-written JSON. |
| `--pace BAUD` | Wire-rate-pace reply *bytes* (e.g. `--pace 19200`). `0` or omitted reproduces today's instant-write behavior. |
| `--turnaround-ms N` | Simulated device-processing/RS-485 turnaround delay applied before the first reply byte. Requires `--pace BAUD` with `BAUD > 0`; defaults to `10` ms. |

### Wire-rate pacing semantics

A real Seplos RS-485 exchange at 19200 8N1 moves **10 bits/byte** (1 start + 8 data + 1 stop).
`write_paced` (`impl_core_pacing.rs`) reproduces that: sleep the turnaround, then write reply bytes
in 8-byte chunks, sleeping `chunk_len * 10 / baud` seconds after each chunk — so total wire time is
`bytes.len() * 10 / baud` seconds regardless of chunk size, and bytes visibly trickle rather than
burst (`TCP_NODELAY` is set so no Nagle coalescing hides it). Unpaced (`--pace` absent or `0`) is one
`write_all` + `flush`, no delay — today's behavior, unchanged.

### The stdin control protocol

While serving a TCP peer, a background thread reads newline-terminated control lines from stdin and
applies them to the running `SeplosSim` between request/reply cycles (never blocking on network
I/O). Verbs (verified against `impl_core_control_parse.rs`, including its
`observed_robot_and_python_control_lines_still_parse` test, which pins every spelling below):

| Verb | Argument(s) | Effect |
|---|---|---|
| `behavior normal` | | `Behavior::Normal` |
| `behavior corrupt-checksum` | | `Behavior::CorruptChecksum` |
| `behavior wrong-address` | | `Behavior::WrongAddress` |
| `behavior silence` | | `Behavior::Silence` |
| `behavior garbage` | | `Behavior::Garbage` |
| `behavior truncated:<n>` | `<n>` = byte count | `Behavior::Truncated(n)` |
| `behavior rtn-error:<code>` | `<code>` decimal or `0x`/`0X`-prefixed hex | `Behavior::RtnError(code)` |
| `faults none` | | Clears every fault flag |
| `faults <name>,<name>,...` | comma-separated `FaultFlags` field names (e.g. `high_voltage_protection`) | Sets exactly the named flags true, all others false |
| `state sample14` | | `BatteryState::sample_14_cell()` (the pre-existing literal spelling, kept working unchanged) |
| `state fault-vector` | | `sample14` baseline + 4 pre-set flags: `high_voltage_protection`, `short_circuit_protection`, `low_capacity_warning`, `charge_current_warning` (also kept working unchanged) |
| `state <key=value ...>` | the §2 spec grammar, bare | Runtime spec grammar via `parse_battery_state_spec` |
| `state set <key=value ...>` | the §2 spec grammar, `set`-prefixed | Same grammar, explicit-prefix spelling |

`state`/`behavior`/`faults` are the only three recognized commands; anything else logs an error to
stderr and is otherwise ignored (never panics, never crashes the connection).

## 4. The cucumber harness

`crates/test-acceptance` — the human-readable Gherkin spec-alignment suite. The simulation feature
area (#226 stage W2) defines simulated Seplos devices **at runtime** from Gherkin tables, through the
same `parse_battery_state_spec` grammar as §2/§3 — never a hand-assembled `BatteryState` literal.

- `tests/features/simulation.feature` — positive/edge: table-driven battery states that must
  round-trip field-for-field through `SeplosSim::respond` + the real `lib_parse` decoder, plus
  bounds (`MAX_CELLS`/`MAX_CELLS+1`), extreme scalar values, the byte-stable `sample14` fixture, and
  the `total=` override seam.
- `tests/features/simulation_faults.feature` — negative: every `Behavior` fault-injection variant
  resolved to its exact typed decode outcome, forced RTN-error codes, and the three auto-NAK request
  classifications.
- `tests/steps/simulation.rs` — the Given/When/Then step definitions; battery states are built only
  through `parse_battery_state_spec`, and every decode goes through the real `lib_parse`/`test_kit`
  crates (no protocol re-implementation).
- `tests/cucumber.rs`'s `SpecWorld.simulation: steps::simulation::SimulationState` holds the scenario
  scratch (spec-build result, typed outcome, RTN code); the shared `state`/`sim` fields (battery
  state / `SeplosSim`) live at the top level, shared with `mapping.rs`/`resilience.rs`.

### Running

```sh
just test-acceptance
# or directly:
cargo test -p test-acceptance --locked --test cucumber

# confirmed-only subset (skip @provisional scenarios elsewhere in the suite — neither
# simulation.feature nor simulation_faults.feature carry that tag today):
cargo test -p test-acceptance --locked --test cucumber -- --tags "not @provisional"
```

`harness = false` on the `cucumber` test target means it is **not** nextest-runnable and is
deliberately its own recipe/CI step — never folded into `just test` or `just test-e2e`.

### Adding a scenario (table format)

`Given a battery state defined by the table:` takes a two-column Gherkin table (`key` / `value`
header row, then one row per spec key) and turns it into spec-grammar text
(`"cells=... temps=... current=..."`) that is parsed by the same `parse_battery_state_spec` as
above. Example (from `simulation.feature`):

```gherkin
Scenario: A mid-discharge battery state round-trips through the simulator unchanged
  Given a battery state defined by the table:
    | key       | value          |
    | cells     | 3701,3699,3702 |
    | temps     | 2951,2961,2971 |
    | current   | -820           |
    | soc       | 455            |
    | remaining | 900            |
    | typical   | 1800           |
    | rated     | 1800           |
  When the adapter polls telemetry
  Then the canonical reading matches the battery state field-for-field
```

A negative/edge example (`simulation.feature`):

```gherkin
Scenario: MAX_CELLS + 1 cells is rejected before a simulator can even be built
  Given a battery-state spec requesting 17 cells
  Then the battery-state spec is rejected
  And the rejection message contains "too many cells"
```

And a fault-injection outcome example (`simulation_faults.feature`):

```gherkin
Scenario Outline: A SeplosSim fault-injection behavior resolves to its typed telemetry outcome
  Given the simulator behavior is <behavior>
  When the adapter polls telemetry
  Then the telemetry outcome is <outcome>

  Examples:
    | behavior              | outcome         |
    | corrupted checksum    | BadChecksum     |
    | a wrong address reply | AddressMismatch |
    | truncated to 10 bytes | BadFraming      |
    | silence               | Silence         |
```

## 5. Harness wiring: QEMU e2e and Yocto UAT

Both harnesses build the same `seplos-tcp-sim` binary (`cargo build -p test-kit --features tcp-sim
--bin seplos-tcp-sim`) and drive it as a TCP **client** dialing into the emulator, which acts as the
server — never the reverse.

### QEMU (MCU/Zephyr) e2e — `bin/zephyr/qemu/qemu-e2e.sh`

```sh
./bin/zephyr/qemu/qemu-e2e.sh up|down|build|sim-bin|run [N]
# or via just:
just qemu-e2e-up / qemu-e2e-down / qemu-e2e-build / qemu-e2e-run [n]
```

`cmd_run` connects the sim (`seplos-tcp-sim --connect 127.0.0.1:5001`) against QEMU's
`-serial tcp:...,server,nowait` socket, early and retry-bounded so the firmware's poll backoff stays
shallow. The `SENTIENT_SIM_PACE` env var (unset by default = today's instant replies) passes through
as `--pace ${SENTIENT_SIM_PACE}`, e.g. `SENTIENT_SIM_PACE=19200` runs the sim at real RS-485 wire
rate for a truer-to-hardware timing run.

The Robot suites (`bin/zephyr/qemu/e2e_*.robot`) drive the sim through
`bin/zephyr/qemu/lib/SeplosSimKeywords.py`, a subprocess controller wrapping the stdin control
protocol (§3). `e2e_common.resource`'s `Standard E2e Setup` keyword calls:

```robotframework
Start Seplos Sim    127.0.0.1:${SEPLOS_UART_PORT}    behavior=${behavior}    state=${state}
```

and individual test cases call `Set Sim Behavior` / `Set Sim Faults` with the exact stdin spellings
from §3, e.g. (`e2e_faults.robot`):

```robotframework
Set Sim Behavior    silence
Set Sim Behavior    corrupt-checksum
Set Sim Behavior    wrong-address
Set Sim Behavior    truncated:20
Set Sim Behavior    garbage
Set Sim Behavior    normal
```

and (`e2e_accuracy.robot`):

```robotframework
Set Sim Faults    high_voltage_protection,short_circuit_protection,low_capacity_warning,charge_current_warning
```

### Yocto UAT — `distro/qemu/yocto-e2e.sh`

```sh
./distro/qemu/yocto-e2e.sh up|down|build [MACHINE]|sim-bin|run [N] [MACHINE]
# or via just:
just yocto-e2e-run [n]
```

`cmd_run` boots `sentient-image` under `runqemu` (default `MACHINE=qemuarm64`), then connects the
sim against a host-side chardev socket (`--connect 127.0.0.1:${YOCTO_E2E_SERIAL_PORT}`, default port
`15001`) attached to the guest's second UART — the same "sim dials in early, retries until the
socket is listening" pattern as the QEMU harness. MQTT is reached via slirp hostfwd
(`YOCTO_E2E_MQTT_PORT`, default `18830`). `distro/qemu/e2e_telemetry.robot` reuses
`bin/zephyr/qemu/lib/{MqttKeywords.py,SeplosSimKeywords.py,tms_validate.py}` **verbatim** (via
relative `Library` paths in `distro/qemu/e2e_common.resource`) — the same `Start Seplos Sim` keyword
and stdin control protocol, so the schema/behavior taxonomy has exactly one source of truth across
both harnesses. `.github/workflows/uat.yml` builds the sim binary the same way
(`cargo build --locked -p test-kit --bin seplos-tcp-sim --features tcp-sim`) ahead of the Robot run.

## 6. Cross-links

- [#226](https://github.com/High-Velocity-Decisions/sentient-rs/issues/226) — the tracking issue for
  Seplos sim wire fidelity: a real 0x44 fault capture, RTN codes `01`/`05`, a drift mode, and
  wire-rate UAT.
- [`audit-2026-07.md`](../reports/audit-2026-07.md) — S13 (in its Appendix) records what the sim's fidelity
  work landed and attested on this branch, and what's left for #226.
- `crates/test-e2e/tests/seplos_at_rate_e2e.rs` — the at-rate e2e: proves a real pty-backed transport
  survives a `SeplosSim` reply that genuinely trickles in at 19200 baud (not an instant burst),
  degrades a device-reported RTN error to a counted failure (never fatal), and recovers on the next
  cycle.

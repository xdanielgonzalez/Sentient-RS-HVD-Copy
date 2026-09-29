# Demo — disk / JSON-lines telemetry logging (Scope A, #55–#57)

Closes contract Scope A ("local microSD data logging demo" — `docs/audit-2026-07.md:32`, elaborated
in `docs/gap-analysis-meteor-parity.md`'s "Disk / JSON-lines telemetry logging" finding). This is
the runbook for the field-engineer-facing demo: enable `[disk]`, run the binary, watch every
published TMS payload land on local storage as a rotating JSON-lines log — no network hop, no
broker required to be reachable, survives a reboot.

This is a demo/runbook doc, not itself covered by `docs/usage.md`'s Stage 1/2 gate index — link to
it from there (`docs/usage.md` is out of this workstream's file ownership) the way the other
per-feature docs (`docs/seplos-sim.md`, `docs/wire-serialization.md`) are: a one-line pointer under
whichever section covers running the RPi binary.

## What it does

`DiskTap` (`crates/impl-app/src/host/types/disk_tap.rs`) is one of the `SinkSet` taps every
published `TapEvent::Published` is fanned out to (see `crates/impl-app/src/host/impl_host_tap.rs`)
— it forwards the already-rendered TMS topic + payload bytes to `lib_persist::run_writer`
(`crates/lib-persist`), a bounded background task that tails them into a rotating `*.jsonl` log:

- **One JSON object per line:** `{"ts":<ms>,"bus":"<name>","topic":"<mqtt topic>","payload":<verbatim TMS JSON>}`.
  `payload` is the exact rendered JSON as published — embedded byte-for-byte, never re-escaped as a
  string (see `impl_jsonl_writer.rs::format_line`).
- **Black-box recorder:** it logs from the tap fan-out, independent of whether the embedded MQTT
  broker (or a downstream subscriber) is even up — the log keeps growing even with zero MQTT
  clients connected.
- **Bounded, never blocks the poll loop:** offering a record to the writer is a non-blocking
  `try_send`; a full queue drops the newest record and increments a counter
  (`JsonlHandle::dropped`) rather than back-pressuring telemetry.
- **Bounded on-disk footprint:** size-based rotation (`tms.jsonl` → `tms.1.jsonl`, `tms.2.jsonl`,
  …), oldest segment deleted once more than `max_files` are retained. Total footprint is bounded to
  `max_file_bytes * (max_files + 1)` — the active file plus every retained rotated segment. This is
  the property that makes it safe to leave running unattended on a small microSD card.
- **Disk-full (`ENOSPC`) resilience (#56):** on a write failure, the writer deletes the oldest
  rotated segment and retries once; if that still fails it enters a degraded state (drop + count,
  throttled warning, periodic re-probe every `full_probe_ms`) and auto-recovers the next time a
  write succeeds — never panics, never blocks the sender.
- **Fsync discipline (#55):** the active file is `fsync`'d every `fsync_every_lines` lines, on every
  rotation, and once more on a graceful shutdown (drain-on-shutdown) so a power-cut microSD field
  device loses at most a small, bounded tail of unflushed lines.

## Enable it

Add a `[disk]` section to the TOML config (absent by default — presence of the section is what
enables the tap, the same opt-in-by-presence convention as `[meshtastic]`/`[greengrass]`/
`[location]`):

```toml
[disk]
dir = "/var/lib/sentient/telemetry"   # default shown; put this on the microSD-backed mount point
file_prefix = "tms"                   # -> tms.jsonl (active), tms.1.jsonl, tms.2.jsonl, ...
max_file_bytes = 10485760             # 10 MiB per file (default)
max_files = 10                        # + 1 active = 11 files retained max (default)
fsync_every_lines = 100               # default
queue_depth = 256                     # default; channel depth between the poll loop and the writer
full_probe_ms = 5000                  # default; re-probe cadence while disk-full/degraded
```

Every field has the default shown above, so `[disk]` with no fields at all (just the section
header) is a valid, minimal way to turn the tap on.

## Run the demo

`[disk]` (like `[meshtastic]`/`[greengrass]`/`[location]`) is enabled by the *section being
present* in the TOML — put it in the config file itself rather than relying on env vars alone to
conjure a section from nothing (env overrides are for tweaking fields of an *already-present*
section, same as every other `[section]`; see `docs/usage.md`'s "Running the RPi binary"). Copy the
base config and add the section:

```sh
# Point `dir` somewhere you can watch easily for the demo (a tmpfs/scratch dir is fine off-hardware —
# on the real Pi deployment, point it at the microSD-backed mount, e.g. /var/lib/sentient/telemetry).
mkdir -p /tmp/sentient-disk-demo
cp config/sentient.toml /tmp/sentient-disk-demo.toml
cat >> /tmp/sentient-disk-demo.toml <<'EOF'

[disk]
dir = "/tmp/sentient-disk-demo"
EOF

SENTIENT_CONFIG=/tmp/sentient-disk-demo.toml \
SENTIENT_SERIAL__PORT=/dev/ttyUSB0 \
cargo run -p bin-rpi
```

(Without real serial hardware attached, the poll loop still starts in its degraded/disconnected
mode — see `docs/usage.md`/`deployment.md` — so this is still useful for exercising the disk tap on
a dev machine; every emitted TMS payload still lands on disk exactly as it would with a live
device.)

In a second terminal, tail the active file:

```sh
tail -f /tmp/sentient-disk-demo/tms.jsonl
```

Each line is a single JSON object; pretty-print one for inspection:

```sh
tail -n1 /tmp/sentient-disk-demo/tms.jsonl | python3 -m json.tool
```

```json
{
  "ts": 1752600000123,
  "bus": "default",
  "topic": "tms/METEOR-001/dc_measurement_update",
  "payload": { "...": "the exact rendered TMS payload, verbatim" }
}
```

## Watching rotation happen

Rotation is size-based (`max_file_bytes`), so it's easy to force for the demo by overriding it —
this is a plain field override on the `[disk]` section the config file above already has, the same
`SENTIENT_<SECTION>__<FIELD>` convention `SENTIENT_SERIAL__PORT` uses:

```sh
SENTIENT_CONFIG=/tmp/sentient-disk-demo.toml \
SENTIENT_SERIAL__PORT=/dev/ttyUSB0 \
SENTIENT_DISK__MAX_FILE_BYTES=2048 \
SENTIENT_DISK__MAX_FILES=2 \
cargo run -p bin-rpi
```

```sh
watch -n1 'ls -la /tmp/sentient-disk-demo/'
```

You'll see `tms.jsonl` (the active file) plus up to 2 rotated segments (`tms.<N>.jsonl`,
monotonically numbered); once a third rotation happens the oldest surviving segment is deleted —
the directory never grows past `max_file_bytes * (max_files + 1)` bytes.

## Simulating disk-full

The precise `ENOSPC` recovery algorithm (delete oldest segment, retry once, degrade + re-probe) is
unit-tested directly against a faulting backend fake in
`crates/lib-persist/src/impl_jsonl_writer.rs`'s test module — no real full disk needed to verify
it. For a live demo of the *externally observable* behavior (never crashes, never blocks), point
`[disk].dir` at a small dedicated loopback/tmpfs mount and fill it, or simply watch the binary's
logs (`journalctl -u bin-rpi -f` in production, stderr in a dev run) for the throttled
`lib-persist: degrading (...)` / `lib-persist: write condition recovered` log lines as space runs
out and is freed again.

## The microSD narrative (field deployment)

On the shipping Pi image (Yocto `core-image-minimal`, read-only rootfs — see CLAUDE.md "Platform &
Deployment"), `[disk].dir` should point at the microSD's writable data partition. Size
`max_file_bytes`/`max_files` to the card's capacity budget with headroom for the rest of the
image's writable state (logs, the embedded broker's own state, etc.) — the bounded-footprint
property above is exactly what makes this a safe, unattended, long-running field configuration
rather than an unbounded write sink that could eventually fill the card. A field engineer with
physical access to the device (or via `journalctl`) can pull the card, or `scp`/`rsync` the
`*.jsonl` files off over the local network, and get the device's complete recent telemetry history
without ever depending on the cloud/MQTT bridge path being reachable — the point of Scope A.

## Automated coverage

- **Acceptance (black-box, real filesystem):** `crates/test-acceptance/tests/features/disk_logging.feature`
  — every published payload appears as one JSONL line (envelope shape, verbatim payload); the log
  is bounded via rotation with oldest-first deletion; the writer degrades without crashing when its
  storage location cannot be written. Run with `cargo test -p test-acceptance --locked --test
  cucumber` (see `docs/usage.md`; the cucumber binary is `harness = false`, not nextest-runnable).
- **Unit (`crates/lib-persist`):** rotation boundaries + deletion order against a real tempdir,
  JSONL envelope shape (including the malformed-payload defensive fallback), the drain-on-shutdown
  fsync path, and the exact `ENOSPC` recovery algorithm against a faulting `WriteBackend` fake. Run
  with `cargo test -p lib-persist`.

No content in this demo or its automated coverage is provisional/reference-derived — the JSONL
envelope shape, rotation scheme, and disk-full policy are this workstream's own design against the
Scope A contract text (see the "Deviation/caveat" note in the umbrella story's WS handoff for what
*was* reference-blocked in earlier work streams; disk logging has no meteor-adapter behavioral
contract to match against, only the plain-English Scope A requirement).

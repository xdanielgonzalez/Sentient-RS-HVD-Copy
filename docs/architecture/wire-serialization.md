# Wire Serialization — streaming integer JSON on the MCU

How `lib-mapping` turns a `CanonicalReading` / `CanonicalFault` into TMS-compliant JSON on the wire,
and **why the MCU path uses a bespoke flat integer-formatting serializer instead of `serde`.**

## Context

The device maps each canonical reading to the three TMS `@topic` message shapes
(`DcMeasurementUpdate`, `PowerHardwareUpdate`, `ActiveDiagnosticState` — see
[`tms-compliance.md`](../compliance/README.md)) and publishes them as JSON over MQTT. On the RPi that
JSON is produced by `serde-json-core` and there is ample RAM. On the Zephyr MCU
(`qemu_cortex_m3`, **64 KB SRAM**) the device *hosts* the MQTT broker, and the broker thread renders
each payload on demand — so the serialization cost sits on the critical path, on a tight stack.

## The problem `serde` caused

The original path built the full TMS struct (`energy_storage_state_from` materializes
`PowerHardwareUpdate` with its 50-slot cell `Vec`) on the stack, then `serde-json-core` serialized
it. serde's `Serialize` dispatch is **recursive** — one call frame per nesting level:
`PowerHardwareUpdate::serialize` → `EnergyStorageState::serialize` → `Vec::serialize` →
`StorageCellState::serialize` → `serialize_f32` → `ryu::format32`. On the broker thread this reserved
huge frames (`rust_broker_main` **9,664 B**; `render_fault` **14,400 B**) and overran the 16,896 B
broker stack by **2,252 B** — an intermittent, on-target-only stack overflow that overwrote the
return address (root-caused via a symbolized fault dump; see the *Bounded Debugging* rule in
`CLAUDE.md` at the repo root).

## The decision

On the MCU, serialize canonical → TMS JSON with a **bespoke, flat, `no_std` / no-alloc,
integer-formatting serializer** — no `serde`, no `ryu`, no `f32`, no intermediate TMS struct.

1. **Integer formatting, not floats.** Canonical values are already scaled integers (cV, signed cA,
   mV, deci-percent, raw Kelvin×10). Rather than scale to `f32` and let serde/`ryu` produce a
   shortest-round-trip string, format the scaled integer and place the decimal point positionally:
   `5452 → "54.52"`, `-164 → "1.64"` (with the TMS sign inversion), `775 → "0.775"`. No float
   formatting at all.
2. **Constants in FLASH.** The JSON structure and field names are `&'static str` — compile-time
   constants in `.rodata`/flash, costing **zero RAM** (which is what Rust string literals / `write!`
   templates already compile to; no bytecode/alphabet interpreter is needed to get the small
   footprint).
3. **Flat traversal.** A single function with a `for` loop over the cells — bounded, shallow stack
   independent of cell count, no per-field recursion.
4. **One-pass length for the MQTT prefix.** MQTT PUBLISH needs the payload length in the fixed header
   *before* the bytes. The const structure has a compile-time-known length, and each value's length
   is pure arithmetic from its magnitude + sign + fixed fraction digits — so length is computed
   cheaply, then the bytes are streamed. This runs through **one shared traversal** over a `JsonSink`
   trait: a `CountingSink` (sums lengths, emits nothing) and a `WriteSink` (emits bytes) execute the
   *same* serialize code, so the length can never desync from the output (a property test asserts
   `counted == emitted`).
5. **Render on output; store canonical.** The broker keeps compact `CanonicalReading`s in memory (a
   ring-buffer arena) and renders JSON only at transmission — it never stores the ~10×-inflated
   serialized bytes.

## Why (the rationale)

- **Why not `serde-json-core`:** its per-field/element recursion is the stack cost — deep frames
  plus the `ryu::format32` leaf and the materialized struct. Correct and ergonomic on the host; too
  stack-heavy for the MCU broker thread on 64 KB.
- **Why not an off-the-shelf crate:** a 2026-07 survey found *no* crate that is simultaneously
  `no_std`, no-alloc, streams-to-a-sink, **and** flat. The flat imperative writers (`json-writer`,
  `struson`) are std-only; the `no_std` streaming ones (`serde-json-core`, `ser-write-json`) keep
  serde's recursion; `lil-json` fits on paper but is immature/unverified for nesting. The idiomatic
  first-party path — `core::fmt::Write` + a flat `write!` traversal — is a small hand-roll for a
  fixed schema, with full control and a guaranteed flat stack.
- **Why integer formatting specifically:** it removes the float formatter entirely *and* makes the
  serialized length deterministic — exactly what makes the one-pass MQTT length calc possible. The
  stack win comes from removing the serde recursion; avoiding floats is the bonus that unlocks the
  length pre-computation.
- **Why the count/emit shared sink:** an MQTT length prefix that disagrees with the emitted body
  corrupts the packet. Sharing one traversal for both makes divergence impossible by construction;
  the `counted == emitted` property test is the proof.

## Consequences

- **Fixed precision, not shortest round-trip.** Wire bytes differ from the old `ryu` output (e.g.
  `54.5` may render `54.50`) — a valid JSON number, within the accuracy tolerance (±0.005,
  [`tms-compliance.md`](../compliance/README.md)). Golden fixtures are regenerated from the new
  serializer so tests stay consistent.
- **`NaN` → `null`** is a `const` emit (present-but-not-available fields `run_time`,
  `power_rate_of_change`).
- **Host-testable.** The serializer, the `counted == emitted` invariant, and value-correctness are
  all `cargo test` / proptest on the host — the fix is proven off-target before any emulator run.
- **One JSON wire format.** The RPi publish path uses the same serializer, so host and device emit
  identical JSON over MQTT. (The Pi also ships a second, binary wire format for DDS — XCDR2, not
  JSON — see the new section below; this document's serializer is the MQTT/JSON path only.)

Implementation: `lib-mapping` (serializer module) + `transform.rs` scaling/sign rules; the broker
render path in `bin-zephyr-platform` (the Zephyr broker pump) and the `Publisher` sink call it. Field shapes and
units per [`tms-compliance.md`](../compliance/README.md).

## Legacy materializers are test-only oracles (WP8)

The original `ToTms` / `ToDiagnostics` trait materializers (in `lib-mapping::transform`) built
full TMS structs on the stack and are now **test-only**, gated with `#[cfg(feature = "std")]`.
Production code has **entirely** migrated to the wire-serializer path:

- `render_tms` / `render_fault` in `crates/lib-mapping/src/render.rs` — flat, no-struct rendering
- View structs in `crates/lib-mapping/src/wire/` — projection accessors that implement the generated
  `*Src` traits (canonical values only, no materialization)
- Generated identity impls for struct topics (`DeviceInfo`, `Reply`, `Heartbeat`) that read the TMS
  struct through `&self` pointers (zero-copy)

The materializer traits survive **only as cross-oracle verification** — test suites can compare
old serde paths against new wire paths to catch regressions. They are compiled only when tests run
(the `std` feature on the host). The MCU `no_std` build excludes them entirely, recovering ~14.9 KB
stack frames (`to_active_diagnostics` was the worst-case).

## Measured outcome — the direct-write serializer (architectural point)

The 2026-07 chain-serializer rounds (PR #316) replaced every remaining materialize-then-serialize
path with **direct sink writes**: per-field value pointers (`*Src` accessor traits) feed a
compile-time field chain (`lib_tms::fields`, generated from the IDL) that emits straight into the
caller's output buffer. No TMS struct is built, no scratch buffer is staged, nothing is retained.
The measured gains are the standing justification for this architecture:

**MCU stack (thumbv7em, pinned 1.91, `-Z emit-stack-sizes`, `opt-level=z` — the hard gate):**

| symbol | materializing path | direct write |
|---|---:|---:|
| `render_fault` | 7,320 B | **48 B** (−99.3 %) |
| `render_tms` | 1,672 B | **48 B** (−97 %) |
| `to_active_diagnostics` (materializer) | 14,952 B | *gone from the build* |
| `ActiveDiagnosticState` / `Vec<DTC,64>` clones | ~7,300 B each | *gone* |
| `TopicRings::push_active_diagnostic_state` (by-value store) | 4,936 B | *deleted (orphaned)* |
| worst binding frame (`StorageUpdateWire::serialize_flat`) | — | 208 B |

After the rounds, the **worst frame in the entire MCU `no_std` roster is 592 B** (the Seplos frame
parser — parsing, not serialization). The original serde render path reserved **14,400 B** for a
single `render_fault` call and was the root cause of the T3 broker-thread stack overflow; under the
direct-write design that failure class is unrepresentable — there is no struct to occupy a frame.

**Pi hotpath (release e2e, real platform + pty sim + embedded broker, 5,000 cycles):**
`run_cycle` 0.97×, publish-path heap 0.92×, 8.7 µs per rendered publish. (Modest by design — the
battery path was already flat; the Pi's structural win is that serde materialization left every
topic, not just battery. Note these are x86 proxy numbers; a real-Pi bench lane is a noted follow-up.)

**Wire bytes:** the omit-absent policy (MIL-STD-3071 A.9.7.5 / Table A.13) drops 3 `null` keys per
DTC and every absent optional — direct headroom against the 1,664 B `MAX_PAYLOAD` budget.

**The invariants that produced these numbers** (hold them in review): values are read through
borrows of the canonical/source struct at emit time only; names/order/presence come from the
generated chain (a missing binding is a compile error); the sink writes bounds-checked bytes
directly into the caller's buffer; nothing render-side is ever stored (the canonical arena is the
only store — `TopicRings`, which stored rendered-side structs by value, was deleted on this basis).

## DDS/XCDR2: a second wire format, Pi-only (story #321)

Everything above is the MQTT/JSON path — the sole wire format when this document was written. As of
story #321 the Pi (not the MCU) also ships a config-gated DDS/RTPS transport, encoding the *same* TMS
types as **XCDR2 binary** (DDS-XTypes 1.3) instead of JSON — a materially different serializer with
its own concerns (DHEADER framing, 4-byte alignment, positional rather than named fields, no
`serde`-shaped recursion problem to begin with, since `lib-tms`'s hand-written `cdr/` core was
designed flat from the start; generated per-topic chains in `lib-mapping`). The stack-discipline
rationale in this document (flat traversal, no intermediate struct, count/emit shared sink) is
retained in the XCDR2 chains but is a **separate implementation** — `lib-tms::cdr` /
`lib-mapping::render_cdr`, not the `serde-json-core`/flat-`write!` path described above. It does not
run on the MCU (DDS is Pi-only; the MCU stays MQTT/JSON-only per the WP8 spike NO-GO verdict).

Full transport/verification detail — QoS profiles, the vendored `dust_dds` rule-(12) patch, the
CycloneDDS interop evidence, and the honest residual gaps — lives in
[`tms-compliance.md` §7.1](../compliance/README.md#71-transport-mqttjson-and-ddsxcdr2-story-321-wp1-wp7),
not here; this document stays scoped to the MCU JSON-serialization story it was written to explain.

## Considered and deferred: 5-bit template packing

A denser storage scheme was evaluated and **deliberately deferred**: pack the field-name tokens
(`[a-z_]`, 27 symbols) at 5 bits/char — 6 chars per `i32` (30 bits used), 37.5% denser than 8-bit
ASCII. It is a sound technique, but it compresses the constant templates in **FLASH**, and our
binding constraint is **RAM**: the templates are already flash-resident at zero RAM cost, and the
wire must stay standard ASCII JSON for stock-client interop — so it yields **no RAM or bandwidth
reduction** while adding a per-character decode on the emit hot path. Principle: optimize the
constrained resource. Revisit only if a production target becomes flash / code-size bound (the
`lm3s6965` here is the emulation board, not the shipping MCU).

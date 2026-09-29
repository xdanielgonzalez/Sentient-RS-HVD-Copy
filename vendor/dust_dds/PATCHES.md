# Vendored `dust_dds` — SENTIENT patch record

**Upstream:** `dust_dds` **0.15.0** (crates.io / github.com/s2e-systems/dust-dds), Apache-2.0.
**Source of this tree:** the pristine crates.io 0.15.0 crate, copied verbatim.

Two independent SENTIENT patches live here, both required for MIL-STD-3071 / CycloneDDS interop:

1. **rule-(12)** — the XCDR2 NEXTINT delimiter for non-primitive sequences (below).
2. **@optional** (issue #332) — XCDR2 `@optional` struct-member support, so the 7 `@optional`-bearing
   TMS writer topics can serialize (below).

**Modified files** — three, across the two vendored crates; everything else is byte-identical to
upstream (verify: `diff -rq <crates.io-src>/dust_dds-0.15.0/src vendor/dust_dds/src` reports exactly
these, plus the same for `dust_dds_derive`):

- `src/xtypes/serializer.rs` — rule-(12) **and** @optional
- `src/xtypes/deserializer.rs` — rule-(12) **and** @optional
- `../dust_dds_derive/src/derive/type_support.rs` — @optional only

No reformatting sweep; every edit is fenced with `// --- SENTIENT rule-(12) patch ---` or
`// --- SENTIENT @optional patch ---` markers.

## Why

MIL-STD-3071 mandates XCDR2 (DDS-XTypes 1.3). Per **DDS-XTypes 1.3 §7.4.3.5.3 rule (12)**, a
sequence whose element type is **non-primitive** (struct or string elements) is serialized with a
leading **NEXTINT** — a 4-byte `UInt32` DHEADER giving the byte length of `{ length + elements }`:

```
rule (11) primitive elements:      { length:u32 } << element*
rule (12) non-primitive elements:  { NEXTINT:u32 } << { length:u32 } << element*
```

**CycloneDDS 11.0.1 requires the NEXTINT and its decoder REJECTS a sequence without it** (observed
live in the `deploy/dds-conformance/` harness: `DcMeasurementUpdate` failed deserialization —
`"deserialization DcMeasurementUpdate/DcMeasurementUpdate failed"` — while primitive-only `Heartbeat`
decoded cleanly). Stock `dust_dds` 0.15.0 does **not** emit the NEXTINT, so its XCDR2 output is
un-decodable by CycloneDDS for every telemetry topic carrying a struct/string sequence. This is a
known, live interop dispute:

- eclipse-cyclonedds/cyclonedds-cxx **#218** (CycloneDDS emits/requires the delimiter).
- OMG DDS-XTypes erratum **DDSXTY14-56** (clarifies rule (12) NEXTINT for collections).

CycloneDDS is the interop arbiter for this program's done-bar, so `dust_dds` is the outlier and is
patched to conform.

## The patch (surgical, XCDR2-only)

The fix must NOT touch the CDR v1 / PL_CDR wire forms (used by RTPS discovery / parameter lists), so
it is confined to the XCDR2 (`EncodingVersion2` / `CdrVersion2`) code paths.

### `src/xtypes/serializer.rs`
1. Two non-primitive sequence arms of the shared default `serialize_sequence` (`STRING8`,
   `STRUCTURE`) are refactored to call two new **overridable** trait methods
   `serialize_string_sequence` / `serialize_complex_sequence`. Their **default** bodies reproduce the
   original `{ length } << element*` form exactly — so CDR1 / PL_CDR output is byte-unchanged.
2. `impl XTypesSerializer for Xcdr2Serializer` **overrides** both to prepend the NEXTINT: the byte
   length of `{ length + elements }` is measured with a fresh position-0 counting serializer
   (`ByteCounter`), then emitted as a `u32` before the body. The count is position-independent
   because XCDR2 max-align is 4 and the NEXTINT is a 4-byte `u32`, so the counted subtree begins
   4-aligned exactly as in the live stream (identical argument to the existing
   `serialize_appendable_struct` DHEADER). The counter always uses `LittleEndian` because a byte
   COUNT is endianness-independent (mirrors the existing `count_bytes_cdr2`).

### `src/xtypes/deserializer.rs`
1. A new overridable trait method `consume_collection_dheader` (default: no-op) is called at the top
   of `deserialize_string_sequence` and `deserialize_complex_sequence`.
2. `impl XTypesDeserialize for Cdr2Deserializer` **overrides** it to read (and discard) the 4-byte
   NEXTINT. CDR1 / PL_CDR keep the no-op default, so their decode is unchanged.

Net effect: `dust_dds` now round-trips its own XCDR2 bytes with the rule-(12) delimiter, and its
`Cdr2LeSerializer` output is byte-identical to this program's hand-written `lib_tms::cdr` chains
(cross-checked by `crates/lib-mapping/tests/wire_cdr_cross_oracle.rs`) and accepted by CycloneDDS.

## The `@optional` patch (#332, surgical, XCDR2-only)

### Why

7 of the 10 TMS publish topics (`DeviceInfo`, `PowerPortState`, `EnergyStartStopState`,
`ActiveMicrogridControllerState`, `PowerHardwareUpdate`, `StorageUpdate`, `ActiveDiagnosticState`)
carry `@optional` members somewhere in their type closure. `dust_dds_derive` 0.15.0 cannot represent
them: `Option<T>` fields fail under `DataStorageMapping`, and the XCDR2 APPENDABLE/FINAL path writes
**no presence flag** (only MUTABLE / PL_CDR consults `is_optional`, substituting defaults). Per
**DDS-XTypes 1.3 §7.4.3.5.2**, an `@optional` member in a FINAL/APPENDABLE struct is serialized as a
1-byte boolean `<is_present>`, then the value iff present — exactly what this program's hand-written
`lib_tms::cdr` chains emit (`emit_optional`) and what CycloneDDS 11.0.1 both writes and requires.

### `dust_dds_derive/src/derive/type_support.rs`

`#[dust_dds(optional)]` on an **`Option<T>`** field now emits presence-aware codegen: `create_sample`
maps an absent member-key → `None`; `create_dynamic_sample` sets the member key only when `Some`
(mapping `into_storage` against the inner `T`, so no `DataStorageMapping<Option<T>>` is needed);
`default_value` is `None`. The gate is `is_optional && <field type is Option<T>>` — upstream's own
`#[dust_dds(optional)]` on a **bare `T`** field (e.g. `builtin_topics.rs`'s MUTABLE/PL_CDR
default-substitution members) keeps byte-identical upstream codegen. `is_optional` on the member
descriptor and the `Option<T>` `XTypesBinding` are unchanged.

### `src/xtypes/serializer.rs`

- New inherent `Xcdr2Serializer::serialize_members_with_optionals`: iterates **declared** members
  (via `type_ref()`), emitting a 1-byte `<is_present>` boolean (`0x01`+value present, `0x00` absent)
  for every `is_optional` member in declared order; non-optional members are serialized as before.
- `serialize_appendable_struct` (XCDR2 override) routes **both** the DHEADER byte-count pass and the
  real emission through it (the pre-patch dense `0..get_item_count()` index walk is wrong once
  optionals make member ids non-dense and would omit the booleans from the counted length).
- `serialize_final_struct` (XCDR2 override) deliberately **does NOT** use the declared walk — it
  defers to the shared set-member walk (`serialize_all_dynamic_data_members`). Two reasons: (1) no
  FINAL struct in the TMS model is `@optional`-bearing (all are APPENDABLE), so no boolean is ever
  needed there; (2) the RTPS instance-key path (`src/dcps/xtypes_glue/key_and_instance_handle.rs`)
  calls `clear_nonkey_values()` then force-sets the descriptor to **FINAL** before serializing a
  **key-only** `DynamicData` for the instance-handle MD5 — a declared-member walk would try to
  serialize the cleared non-key members (`XTypesError::InvalidId` on every write) and splice spurious
  presence bytes into the key hash, breaking CycloneDDS instance-handle interop. The set-member walk
  serializes exactly the populated key. (This subtlety was a real regression; see the byte-identity
  **and** live-writer coverage in `crates/lib-dds/tests/optional_topics_wire_identity.rs` +
  `crates/test-e2e/tests/dds_interop_e2e.rs`.)

### `src/xtypes/deserializer.rs`

- New overridable `consume_optional_presence` (default: reads nothing, returns `true`) — so CDR1 /
  PL_CDR decode is byte-unchanged.
- `deserialize_final_member` (reached by both FINAL and, via its DHEADER, APPENDABLE) guards on
  `is_optional`: consumes the presence boolean and, when absent, returns without any `set_*_value`,
  so the member key stays unset (→ `None` on `create_sample`).
- `Cdr2Deserializer` overrides `consume_optional_presence` to read the 1-byte boolean.

Net effect: all 10 writer topics serialize/deserialize their XCDR2 form byte-identically to the
`lib_tms::cdr` chains, present **and** absent optionals — proven for the 7 `@optional` topics by
`crates/lib-dds/tests/optional_topics_wire_identity.rs` (8 tests: byte-identity + round-trip + a
DeviceInfo CycloneDDS golden decode) and end-to-end over the real transport by
`crates/test-e2e/tests/dds_interop_e2e.rs`.

## How it is consumed

Via `[patch.crates-io]` **path** override in the workspace `Cargo.toml` (a git source is forbidden by
`deny.toml` `unknown-git = "deny"`; a path patch keeps the source registry-clean). Both `dust_dds`
and its `dust_dds_derive` proc-macro are vendored so the pinned pair resolves together.

## Removal plan

Drop `vendor/dust_dds` + `vendor/dust_dds_derive` and the `[patch.crates-io]` block once the fix is
released upstream. Upstream PR draft: `.scratch/dust-dds-upstream-pr.md`.

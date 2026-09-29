# `lib-mapping` — TMS (MIL-STD-3071) payload mapping

Vendor-neutral canonical reading → TMS typed structs → `no_std` JSON, plus the `Publisher` publish
contract. The standards-facing middle of the pipeline.

## Role in the pipeline

```
CanonicalReading / CanonicalFault ──▶ lib-mapping ──▶ TMS JSON payloads ──▶ Publisher
```

The mapper consumes only `lib_parse::CanonicalReading` / `CanonicalFault` (never a vendor's own
types). Structs are authored from the authoritative standard IDL (MIL-STD-3071 Change 2, TMS Data
Model v2.0.3), not the Python prototype. Schema-shape compliance vs the standard is documented
separately in [`../tms-compliance.md`](../../compliance/README.md).

## `no_std` status

`#![cfg_attr(not(feature = "std"), no_std)]` + `#![forbid(unsafe_code)]`. Pure, zero I/O. Topics are
built into a bounded `heapless::String`; payloads serialize via a `no_std` JSON codec
(`serde-json-core`) into a stack buffer. Features: `default = ["std"]`; `std` pulls in
`lib-parse/std` + `serde/std`.

## Key public types

| Item | What it is |
|---|---|
| `schema` | TMS typed structs + enums + topic-name constants — the isolated swap-seam for the standard. Covers the STOR (energy-storage) subset: `DcMeasurementUpdate`, `PowerHardwareUpdate` (carrying `EnergyStorageState`), `ActiveDiagnosticState`. A re-export of the centralized [`lib-schema`](./lib-schema.md) crate (`pub use lib_schema as schema;`), not defined in this crate. |
| `schema::Identity` | `tms::Identity` = `heapless::String<32>` (device id, `@key`). |
| `schema::ClockMonotonic` | Monotonic timestamp (seconds + nanoseconds). |
| `schema::topics` | Topic-name constants (`DC_MEASUREMENT_UPDATE`, `POWER_HARDWARE_UPDATE`, `ACTIVE_DIAGNOSTIC_STATE`). |
| `transform` | Canonical → TMS: sign inversion (vendor charging-positive → microgrid convention), mV→V, temperature aggregation, fault → DTC. Exposes `ToTms` / `ToDiagnostics` conversion traits. |
| `codec` | `no_std` JSON encode via `serde-json-core` (`to_json_slice`). |
| `sink::Publisher` | The MQTT publish sink trait (`publish`, `healthy`) — `no_std`-safe signature. |
| `sink::publish_tms` | Serialize a TMS payload to JSON and publish to `{prefix}/{device_id}/{snake_case(topic)}`, no allocation. |
| `sink::topic_into` | Bounded CamelCase→snake_case topic builder. |
| `sink::PublishError` | `Serialize` / `TopicTooLong` / `Sink`. |

Floats default to `NaN` per the standard (`@optional`/not-available), not zero. Sequence bounds
mirror the IDL MAXLENs (DC line ≤ 3, storage cells ≤ 50, DTC ≤ 64).

AWS-Greengrass backwards-compat is a flat legacy schema relayed by the mosquitto bridge
(`distro/deploy/mosquitto.conf`), **not** a module here.

## Dependencies and dependents

- **Depends on:** `lib-parse` (canonical types), `lib-schema` (the `schema` re-export, the
  centralized TMS typed structs — see [`lib-schema`](./lib-schema.md)), `bytes`,
  `heapless` (with `serde`), `serde`, `serde-json-core`.
- **Depended on by:** `lib-mqtt`, `impl-app`. The `Publisher` contract it defines is
  re-exported by `lib-mqtt` and implemented by both the RPi broker and the Zephyr MQTT client.

## Testing

- `tests/tms_transform.rs` — canonical → TMS transform assertions.
- `tests/codec.rs` — the `no_std` JSON codec.
- `tests/tms_roundtrip_proptest.rs` — property-based TMS round-trip (`proptest` + `serde_json`
  dev-deps to parse the emitted JSON back).

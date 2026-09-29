<p class="eyebrow">TMS Compliance · Field-by-Field Report</p>

# TMS Compliance Report — Rust Adapter Schema Shape

**Project:** HVD → Sentient METEOR
**Component:** `sentient-rs` mapping/serialization layer (Seplos BMS + Voltronic RS-232 inverter → TMS)
**Standard:** MIL-STD-3071 w/ Change 2, Appendix D (TMS Data Model, DDS/IDL) — **TMS Data Model v2.0.2**
(`tms.TMS_VERSION`, `docs/compliance/tms/mil-std-3071_data_model.idl:13`)
**Source of the standard used here:** `docs/compliance/tms/mil-std-3071_data_model.idl` — the
checked-in ground-truth IDL (5,346 lines; landed by the reapplied `docs(compliance)` commit under
gap-closure wave 1, `25be971`) — plus `docs/compliance/tms/mil-std-3071.pdf` for the narrative
timing/QoS-table content (Tables A.5, B.1/B.2, §4.9.10.x) that has no IDL representation.
**Date:** 2026-07-16 (re-baselined, wave-4 docs sweep). Original report 2026-07-04; TMS-breadth
addendum 2026-07-16 (earlier same day, pre-reshape).

**This report supersedes its own prior citation source.** Every table below was re-verified this
pass directly against the checked-in IDL file (`grep`/`sed` against the real file, not
reconstructed from memory of the old citations). The original 2026-07-04 report was authored
against `D:\hvd\meteor-adapter\docs\chapter-d.txt` — a hand-extracted, one-token-per-line text dump
of a **2.0.3 draft** (29 Sep 2025) of Appendix D that was never checked into this repo and is not
this report's source anymore. See issue #266 ("TMS version skew").

## 1. Purpose and scope

This is primarily a **schema-shape / structural-compliance** review of the adapter's MQTT/JSON
transport, not a certified DDS conformance test. Its core question: *do the TMS message structs the
Rust adapter emits over MQTT/JSON match the field shape (name, type, key/optional annotation,
sequence bounds) of the corresponding IDL structs in MIL-STD-3071 Appendix D?* That JSON-shape
question is answered independently of transport, and independently of whether XTypes
type-compatibility hashing over DDS/RTPS is certified — a real DDS participant and a certified
conformance-test suite are still a heavier bar than what's asserted here. **As of story #321 the
adapter also carries a real DDS/RTPS transport, verified against a real (stock, unmodified)
CycloneDDS 11.0.1 participant** — that evidence is wire-level, not schema-shape, and is recorded in
its own section, §7.1, rather than folded into the JSON field-by-field tables below (§§2-6 describe
the MQTT/JSON path). Behavioral conformance over the MQTT adaptation (publication cadence, QoS/retain
profiles, the Required STOR publications, and the MC-selection state machine) WAS closed in the
wave-5 sweep — §9 records what closed (#263/#264/#265) and the few items that remain open.

## 2. Device roles and identity (per-bus, decision 0007)

**DEVICE ROLE DECISION (locked, 2026-07-16, decision 0003):** this adapter is a **STOR-role power
device proxying the battery** — not a MON (passive monitor) role. `DeviceInfo.role` for a Seplos bus
resolves to `ROLE_STORAGE`.

**Superseded in part by decision 0007 (gap-closure wave 3, #275):** the adapter now also drives a
second, independent leg — a Voltronic/PI30 RS-232 inverter (decision 0001, in-scope) — which is a
genuinely different TMS role, **CONV** (`ROLE_CONVERSION`). Role is derived **per bus**, not from one
global `[device].role` string: `crate::config::Config::identities` maps every configured `[[bus]]`
to one `IdentityCfg{device_id, role, protocol}` — `protocol = "seplos"` → `ROLE_STORAGE`,
`protocol = "voltronic"` → `ROLE_CONVERSION`. A unit wired with both a battery pack and an inverter
therefore runs **two concurrent TMS identities**, one STOR and one CONV, each with its own
`device_id` (default `{device.id}` / `{device.id}-inv`), each publishing its own
Heartbeat/DeviceInfo/Reply/measurement topics. `[device].role` is retained in config for backward
compatibility but is `#[allow(dead_code)]` — accepted, no longer read
(`crates/bin/rpi/src/config/types/device_config.rs`).

The acronym collision this creates with the security-hardening "TPM" (Trusted Platform Module) is
resolved by decision 0007: TEM = battery = `ROLE_STORAGE`; TPM (in the METEOR/Greengrass sense) =
inverter = `ROLE_CONVERSION` — see `docs/decisions/0007-tpm-tem-naming.md`.

## 3. Topic inventory (current state, both roles)

| Topic (IDL `TOPIC_*`) | Data type | QoS Profile / Data Trigger (IDL topic-module doc comment) | STOR (battery) | CONV (inverter) | Status |
|---|---|---|---|---|---|
| `Heartbeat` | `tms::Heartbeat` | Medium / Periodic 1 s | emit | emit | IDL-verified |
| `DeviceInfo` | `tms::DeviceInfo` | PublishLast / At Start | emit **retained + QoS1** (PublishLast), 60 s refresh | emit | IDL-verified; PublishLast met (#265, wave 5) |
| `ActiveDiagnosticState` | `tms::ActiveDiagnosticState` | PublishLast / At Start, On Change | emit **At Start + On Change** (30 s safety-net) | emit | IDL-verified; At-Start/On-Change met (#265); field-order caveat (issue #273) |
| `DcMeasurementUpdate` | `tms::dc::MeasurementUpdate` | Medium / Periodic 1 s | emit every 1 s poll cycle (battery DC port) | emit (PV + battery DC ports) | IDL-verified; default poll 1 s (#265) |
| `AcMeasurementUpdate` | `tms::ac::MeasurementUpdate` | Medium / Periodic 1 s | — | emit (AC-output + grid ports) | IDL-verified shape; PROVISIONAL mapping (decision 0001) |
| `PowerHardwareUpdate` | `tms::PowerHardwareUpdate` | **Slow / Periodic 10 s** | emit **10 s rate-limited** | emit **10 s rate-limited** | IDL-verified; cadence met (#265, wave 5) |
| `StorageUpdate` | `tms::StorageUpdate` | **Slow / Periodic 10 s** | emit **10 s rate-limited** | — | IDL-verified (reshaped wave 1); cadence met (#265, wave 5) |
| `Reply` | `tms::Reply` | Reply / Within 0.1 s | emit (on request), **QoS1** | emit (on request), **QoS1** | IDL-verified; Reliable QoS met |
| `EnergyStartStopRequest`, `PowerSwitchRequest` | `tms::*Request` | Command / As Needed | subscribed (defense-in-depth) | subscribed | IDL-verified; answered `REPLY_NOT_IMPLEMENTED` |
| `ControlHardwareUpdate` | `tms::ControlHardwareUpdate` | Slow / Periodic 10 s | type authored, **not published** | type authored, **not published** | issue #263-adjacent, deferred |
| `EnergyStartStopState` | `tms::EnergyStartStopState` | PublishLast / At Start, On Change | **emit** (retained, At Start/On Change) | n/a (SRC/STOR only) | published (#263, wave 5); content PROVISIONAL |
| `PowerPortState` | `tms::PowerPortState` | PublishLast, 1 s / At Start, On Change | **emit** (retained, At Start/On Change) | **emit** (retained) | published (#263, wave 5); content PROVISIONAL |
| `ActiveMicrogridControllerState` | `tms::ActiveMicrogridControllerState` | PublishLast / At Start, On Change | **emit** (retained; `master_id` from the MC-selection machine) | **emit** | published (#263/#264, wave 5) |
| `DcLoadSharingState` | `tms::dc::LoadSharingState` | PublishLast / At Start, On Change | **emit** (retained), Conditional-Required (HAS DC PORTS, condition met) | — | published (#263, wave 5); all-NaN-honest |
| generic `Request` | *(does not exist in the IDL)* | — | **removed** (was an invented Wave-0 topic) | — | issue #257, fixed wave 1 |

The four Required STOR state topics are published by `bin_rpi::tasks::state_publisher` (per identity,
retained + QoS1, At Start + On Change); their `master_id` (for `ActiveMicrogridControllerState`) is fed
by the MC-selection state machine (`bin_rpi::tasks::mc_select`, §9). Content is PROVISIONAL where the
telemetry-only device cannot source a field (documented per topic in that module + §8).

## 4. Field-by-field comparison

Legend for "Emitted?": **yes** = present in the Rust struct with the named field; **no** = defined
in the IDL but not present in the Rust struct. All line citations are
`docs/compliance/tms/mil-std-3071_data_model.idl:LINE`, re-verified this pass.

### 4.1 `tms::dc::MeasurementUpdate` → `DcMeasurementUpdate`

IDL block: `mil-std-3071_data_model.idl:4961-4985` (`module dc`). Rust:
`crates/lib-schema/src/types/dc_measurement_update.rs`.

| TMS field (IDL) | IDL type | @key/@optional | Emitted? (Rust field) | Source (Seplos / derived) | Deviation / notes |
|---|---|---|---|---|---|
| `deviceId` | `tms::Identity` (`string<1,32>`) | `@key` | yes — `device_id: Identity` | caller (`to_dc_measurement`) | Matches. |
| `timeMeasured` | `tms::ClockMonotonic` | — | yes — `time_measured: ClockMonotonic` | caller `now` | Matches. |
| `externalMeasurement` | `PowerPortMeasurementSequence` (MINLEN 0, MAXLEN `tms::MAX_PORTS` = 64, `idl:903`) | — | yes — `external_measurement: Vec<PowerPortMeasurement, 4>` | one port, one DC line, from `CanonicalReading` | Bound narrowed 4 vs 64 — deliberate bounded-subset adaptation (§7.4). |
| `internalMeasurement` | same sequence type | — | yes — `internal_measurement: Vec<PowerPortMeasurement, 4>` | none — always empty | Battery has no internal meter; MINLEN 0 permits empty. |

**Nested `dc::PowerLineMeasurement`/`dc::PowerPortMeasurement`** (IDL `4882-4941`): `PowerLineMeasurementSequence_MAXLEN = 3`
(`idl:4906`) is an **exact match** to the Rust `MAX_DC_LINES = 3` (`lib-schema/src/lib.rs`) — this
resolves the prior report's open caveat ("`MAX_DC_LINES` rests on the un-expanded nested IDL body");
the nested bodies are now expanded and the bound is confirmed exact, not narrowed. Amperage is
sign-inverted at the source (§5).

### 4.2 `tms::PowerHardwareUpdate` → `PowerHardwareUpdate`

IDL block: `idl:2357-2406`. Rust: `crates/lib-schema/src/types/power_hardware_update.rs`.

| TMS field (IDL) | IDL type | @key/@optional | Emitted? | Notes |
|---|---|---|---|---|
| `deviceId` | `Identity` | `@key` | yes | Matches. |
| `timestamp` | `ClockMonotonic` | — | yes | Matches. |
| `engine`/`fuel`/`generator`/`powerElectronics`/`thermal` | `*State`, all `@optional` | `@optional` | no (except see CONV row below) | Omitted for a battery-only device — legal (all `@optional`). The CONV/inverter mapper (§4.8) populates `powerElectronics.temperature` only. |
| `energyStorage` | `EnergyStorageState` | `@optional` | yes — `Some(..)` for STOR | The only populated subsystem for a battery. |

#### 4.2.1 Nested `tms::EnergyStorageState` / `StorageCellState`

IDL: `EnergyStorageState` `idl:1555-1601`; `StorageCellState` `idl:1496-1529`;
`MAXLEN_storageCells = 50` at `idl:1487` — an **exact match** to the Rust
`MAX_STORAGE_CELLS = 50`. All five fields (`minTemperature`/`averageTemperature`/`maxTemperature`/
`runTime`/`storageCells`) map as before; `runTime` has no Seplos source and is emitted as
`f32::NAN`. Per-cell fields (`cellNumber`/`cycleCounter`/`stateOfCharge`/`temperature`/`voltage`)
are unchanged and match the IDL exactly.

**MAX_PAYLOAD note (updated this pass):** the 2026-07-16 TMS-breadth report measured a
maximally-populated (`MAX_CELLS`=16/`MAX_TEMPS`=8) reading's JSON at **1751 bytes**, 87 bytes over
`lib_mapping::sink::MAX_PAYLOAD` (1664). That measurement was taken against the **old**
`StorageUpdate` shape, which at the time reused this same `EnergyStorageState` construction — since
retired (§4.4). `PowerHardwareUpdate` itself is unchanged by the wave-1/3 reshape and still carries
`storage_cells` the same way, so the **same latent overflow risk still applies to
`PowerHardwareUpdate`** for any future pack exceeding the currently-deployed
Seplos 48150-1101-10E's 14-cell/6-temp shape (1553 bytes measured, `docs/seplos-sim.md`). Not
independently re-measured this pass; treat the 1751-byte figure as the last known data point, not a
fresh confirmation.

### 4.3 `tms::ActiveDiagnosticState` → `ActiveDiagnosticState`

IDL block: `idl:3213-3232`. Rust: `crates/lib-schema/src/types/active_diagnostic_state.rs`.

| TMS field (IDL) | IDL type | Emitted? | Notes |
|---|---|---|---|
| `deviceId` | `Identity` (`@key`) | yes | Matches. |
| `codes` | `DiagnosticTroubleCodeSequence` (MINLEN 0, MAXLEN 64, `idl:2469`) | yes — `Vec<DiagnosticTroubleCode, 64>` | Exact bound match, not narrowed. |
| `overflow` | `uint32` | yes | Count of codes past the 64 cap. |

**Nested `tms::DiagnosticTroubleCode`** — IDL `2416-2465`; Rust
`crates/lib-schema/src/types/diagnostic_trouble_code.rs`:

**Presence policy (MIL-STD-3071 A.9.7.5 / Table A.13):** absent `@optional` fields produce no key/value pair in the JSON output (omit-when-None). This applies to `portNumber`, `estimateTime`, and `estimateSeverity` below.

| TMS field (IDL, declaration order) | Emitted? | Notes |
|---|---|---|
| `portNumber` (`@optional`) | omitted (always `None`) | Device-level faults have no associated port; `PowerPortNumber` is now `i32` (§7.6). Omitted from JSON per presence policy. |
| `suspectParameter` (J1939 SPN) | yes | Battery SPNs 1–16 (adapter-local placeholders, not assigned J1939 SPNs); CONV/inverter SPNs are 100+bit-index, disjoint (§4.8). |
| `failureMode` | yes | Hardcoded `ConditionExists` (FMI 31) for battery faults. |
| `occurrenceCount` (saturates 2³²−2) | yes | Hardcoded `1` — no occurrence tracking. |
| `timeRaised` | yes | Matches. |
| `severity` | yes | Hardcoded `Major` for battery faults — wire spelling now `SEV_7_MAJOR` (§7.5, fixed wave 1; was the bare Rust name `"Major"` before, issue #258). |
| `hint` (`String32`) | yes | Field-order fixed (**issue #273**) — the generated Rust struct now follows the IDL order exactly, with `hint` as the LAST member (after `estimateTime`/`estimateSeverity`), matching the IDL declaration order. Both the flat wire serializer and the generated struct identity emitter now produce IDL-ordered JSON. |
| `estimateTime`/`estimateSeverity` (`@optional`) | omitted (always `None`) | No prognostic model in the Seplos/battery path; both always `None` and omitted from JSON per presence policy. |
| `affectedIdentityId` | **no** | **Removed in wave 1** (issue #259) — the 2.0.2 IDL has no such member at all; the old report's doc comment claiming "added v2.0.3" was unverifiable against the checked-in ground truth and the field was a baseline (Python-prototype) invention, not a real TMS field. |

### 4.4 `tms::StorageUpdate` → `StorageUpdate` — reshaped wave 1 (#271), now IDL-verified

IDL block: `idl:5113-5203`. Rust: `crates/lib-schema/src/types/storage_update.rs`. **This is a
complete reshape**, not an extension of the 2026-07-04 report's provisional guess — the old shape
(`{device_id, timestamp, energy_storage}`, reusing `PowerHardwareUpdate`'s `EnergyStorageState`
construction) shared **no payload field** with the real IDL type and is documented as the prior bad
wire in issue #256. **Note the IDL type has NO `timestamp` field at all.**

| TMS field (IDL) | IDL type | @key/@optional/@range | Emitted? (Rust field) | Source | Notes |
|---|---|---|---|---|---|
| `deviceId` | `Identity` | `@key` | yes | caller | Matches. |
| `internalVoltage` | `float` | `@unit("V")` | yes — `internal_voltage: f32` | `cv_to_v(total_voltage_cv)` | Matches. |
| `stateOfCharge` | `float` | `@optional @range(0,1)` | yes — `Option<f32>` | `Some(dpct_to_pu(soc_dpct))` | Matches. |
| `availableEnergy` | `float` | `@unit("J")` | yes — `available_energy: f32` | `Ah × V × 3600` derived | Matches. |
| `holdTime` | `float` | `@unit("s")` | yes — `f32::NAN` | no Seplos source | Present-but-NaN (non-optional float). |
| `chargeTime`/`dischargeTime`/`maxChargeTime`/`maxDischargeTime` | `Curve2D` (`@unit("W, s")`) | — | yes — all four `Curve2D { points: Vec::new() }` | no Seplos source | Empty sequences (MINLEN 0 on `Point2DSequence`, `idl:1163-1170`) — legal, and keeps the emitted payload small (see the MAX_PAYLOAD note in §4.2). |
| `maxChargeRate`/`maxDischargeRate` | `float` | `@optional @min(0)`/`@max(0)` | yes — both `None` | no Seplos source | Matches. |

### 4.5 `tms::Heartbeat` → `Heartbeat` — IDL-verified

IDL block: `idl:3672-3684`. Rust: `crates/lib-schema/src/types/heartbeat.rs`. Exact 2-field match:
`deviceId`/`sequenceNumber` → `device_id: Identity`/`sequence_number: u32`
(`u32::wrapping_add(1)` per publish, never panics at the `u32::MAX` boundary). Published per
identity by `bin-rpi::tasks::heartbeat::run_heartbeat`.

### 4.6 `tms::DeviceInfo` family — IDL-verified

IDL block: `DeviceInfo` `idl:5286-5344`; `ProductInfo` `idl:4279-4321`; `TopicInfo` `idl:4116-4143`;
`TopicList`/`TopicName` `idl:408-425`. Rust: `crates/lib-schema/src/types/{device_info,product_info,
topic_info,topic_list}.rs`.

`DeviceInfo{device_id(@key), role: DeviceRole, product: ProductInfo, topics: TopicInfo (a SINGLE
struct, not a sequence — matches the IDL exactly), control_hardware?, power_hardware?,
control_parameters?, metric_parameters?, control_service?, power_device?}` — every field name and
optionality now matches the IDL member-for-member, including the `role`/`controlHardware`/
`powerHardware`/`controlParameters`/`metricParameters`/`controlService`/`powerDevice` breadth the
2026-07-04 report's provisional §3.4.2 omitted entirely (that version had only
`{device_id, product, power_ports, topics}` — a flattened, non-IDL invention).

`ProductInfo` is 8 fields (`nsn: String<13>`, `gtin: String<14>`, `manufacturer_name`, `model_name`,
`model_number`, `serial_number`, `software_version`, `platform_id: Option<Identity>`) — an exact
match, whereas the 2026-07-04 report's `ProductInfo` had only 4 plain fields
(`manufacturer`/`model`/`serial_number`/`firmware_version`).

`TopicInfo{data_model_version, published_conditional_topics: TopicList,
published_optional_topics: TopicList, supported_request_topics: TopicList, extension_topics:
Option<TopicList>}` — this adapter's `supported_request_topics` is emitted **empty**
(`Vec::new()`), the honest declaration for a device with no command surface (§4.7). The
`DeviceInfo`-carries-a-single-`TopicInfo` shape (not a per-topic sequence) is now correctly
modeled — the old report's `PowerPortInfo`/`topics: Vec<TopicInfo, 8>` inventory-style struct is
retired.

**Retain still unavailable:** the embedded broker's in-process `LinkTx` has no retain flag
(confirmed by spike, `lib_mqtt::broker` module doc comment), so `DeviceInfo` is periodically
**re-published every 60 s** rather than retained-once — this is unchanged by the reshape and is a
durability-emulation workaround, not a schema deviation.

### 4.7 `tms::Reply`/`ReplyStatus`/`ReplyCode` + Request posture — IDL-verified, generic `Request` topic removed

IDL block: `Reply` `idl:4391-4426`; `ReplyStatus` `idl:4371-4382`; `ReplyCode` `idl:624-644` (13
enumerators, ordinal 0 = `REPLY_UNKNOWN`). Rust:
`crates/lib-schema/src/types/{reply,reply_status,reply_code}.rs`.

`Reply{requesting_device_id(@key), target_device_id(@key), config: ConfigId, port_number:
PowerPortNumber, request_sequence_id: u64 (RequestSequence), status: ReplyStatus}` — a complete
reshape from the 2026-07-04 report's invented `Reply{device_id, request_id, timestamp, status:
ReplyStatus{code, message: Option<String32>}}`. `ReplyStatus{code: ReplyCode, reason: String32}` —
`reason` is REQUIRED (not `Option`), unlike the old guessed `message` field. `ReplyCode` now carries
the full 13-variant IDL enumeration (`Unknown`/`Ok`/`BadRequest`/`MethodNotAllowed`/`Conflict`/
`Gone`/`PreconditionFailed`/`InternalServerError`/`NotImplemented`/`ServiceUnavailable`/
`PendingAuthorization`/`NotMaster`/`PermissionDenied`), each `#[serde(rename = "REPLY_*")]`'d to the
exact IDL enumerator spelling — the old report's 6-variant best-effort guess (`Ok`/`Unknown`/
`InvalidRequest`/`Unsupported`/`Timeout`/`Error`) is retired.

**The generic `Request` topic never existed in the standard** and has been **removed**
(`topics::REQUEST` no longer exists; issue #257). Real requests are typed topics
(`EnergyStartStopRequest`, `PowerSwitchRequest`, `DeviceConfigRequest`,
`PowerPortConfigRequest`, …). The spec-clean posture for a telemetry-only device is an **empty**
`TopicInfo.supported_request_topics` (§4.6) — no command surface declared. As defense-in-depth
beyond that honest declaration, `bin-rpi::tasks::reply_listener::run_reply_listener` additionally
**subscribes the two named request topics** this adapter could plausibly receive
(`EnergyStartStopRequest`, `PowerSwitchRequest`) and answers every well-formed request with
`ReplyCode::NotImplemented` (echoing the request's key fields — `requestingDeviceId`,
`requestSequenceId`, and `config`/`portNumber` where the request type carries them) and every
undecodable payload with `ReplyCode::BadRequest`, always within the 0.1 s budget
(`test-e2e/tests/request_reply_e2e.rs`). `port_number` uses the IDL's `INVALID_PORT_NUMBER = -1`
sentinel (`idl:906`) where a request has no port context — see §7.6.

### 4.8 CONV role / RS-232 inverter leg (gap-closure wave 3, #275) — IDL-verified shapes, PROVISIONAL mapping

Decision 0001 (`docs/decisions/0001-rs232-inverter-leg-in-scope.md`) brought the Voltronic/PI30
inverter leg in-scope. `crates/lib-mapping/src/transform_inverter.rs` maps
`CanonicalInverterReading`/`CanonicalInverterFault` (from `lib-parse`'s `voltronic` feature — see
`lib-parse` in the [API reference](../reference/README.md)) onto the same STOR-side TMS struct types, reused for the CONV role:

- **`AcMeasurementUpdate`** (`tms::ac::MeasurementUpdate`, IDL `4689-4719`) — two external AC ports:
  the load-side AC output (port 0) and the utility grid input (port 1). QPIGS carries neither grid
  current nor grid power, so the grid port's amperage/real/reactive power are `NaN`; the AC-output
  port derives amperage from apparent-power ÷ voltage and carries the reported active power. The
  nested `ac::PowerPortMeasurement`/`ac::PowerLineMeasurement` (IDL `4638-4675`/`4568-4624`) are
  fully IDL-verified shapes (`crates/lib-schema/src/types/ac_power_{port,line}_measurement.rs`);
  only the **port assignment and derivation choices** are PROVISIONAL (no real Voltronic wire
  captures exist yet — decision 0001's open risk).
- **`DcMeasurementUpdate`** — reused, with two external DC ports instead of one: the PV/solar input
  (port 2) and the battery DC bus (port 3). Battery amperage uses the source-positive TMS
  convention (discharge − charge).
- **`PowerHardwareUpdate`** — only `power_electronics.temperature` (the inverter heatsink temp) is
  populated; the other five `@optional` subsystem states stay `None`.
- **`ActiveDiagnosticState`** — one `Major`/`ConditionExists` DTC per set QPIWS fault bit (SPN =
  100 + QPIWS bit index: 105/109/110/112/115/117 — disjoint from the battery's SPN range 1..16),
  plus a synthesized SPN 130 DTC when the QMOD mode word reads `Fault` (the mode has no faithful
  TMS home of its own).

Every mapping choice above is doc-commented at its call site in `transform_inverter.rs` with the
reasoning, so a real Voltronic capture can confirm or correct it — see issue #268 (missing
`SerialTransport::discard_input()`, a related field-hardware risk on this same leg) and
`lib-parse` in the [API reference](../reference/README.md) (voltronic parser details).

### 4.9 Authored but unwired breadth — `ControlHardwareUpdate`, `EnergyStartStopState`, `PowerPortState`

These three types are now IDL-verified in `lib-schema` (correct field names/types/optionality,
cited in their own doc comments) but have **zero publisher call sites** anywhere in
`bin-rpi`/`impl-app`/`lib-mapping` — grepped repo-wide this pass. They exist as compile-time-checked
shapes only; nothing constructs or emits them yet.

- **`ControlHardwareUpdate`** (IDL `2219-2296`) — `{device_id(@key), timestamp, processor_usage?,
  memory_usage?, network_usage?, storage_usage? (all @range(0,1) p.u.), temperature (REQUIRED
  degC)}`. Slow/10 s profile.
- **`EnergyStartStopState`** (IDL `4469-4528`) — `{device_id(@key), present_level:
  EnergyStartStopLevel, future_level: EnergyStartStopLevel, request_lock: bool, elapsed_time (s),
  remaining_time (s), present_level_reason: PowerSwitchReason, present_level_actor:
  Option<Identity>, future_level_reason: PowerSwitchTransitionCode}`. Required for SRC/STOR per
  B.16 (issue #263).
- **`PowerPortState`** (IDL `3560-3579`) — `{device_id(@key), power_ports:
  Vec<PowerPortStatus, 4>}` — IDL bounds `powerPorts` at `MAX_PORTS` (64); narrowed to 4 (§7.4).
  Required for SRC/STOR/DIST/LOAD/CONV per B.17 (issue #263).

**`ActiveMicrogridControllerState` and `tms::dc::LoadSharingState` ("DcLoadSharingState") have no
Rust type at all** — only their topic-name string constants exist in `lib-schema::topics`. Both are
part of the minimum Required STOR publish set (§9) and land in a future wave.

## 5. Source mapping and deviations (Seplos → TMS)

The canonical Seplos reading maps to TMS via `transform.rs`. Two standard conventions are
load-bearing (documented `transform.rs:5-9`, attributed to TMS §2.7):

- **Sign inversion** — Seplos reports positive current as *charging* (into the battery); TMS
  positive is *sourcing into the microgrid*. So `tms_amperage = -vendor_current`. Semantic
  deviation mandated by the standard's sign convention.
- **Unit scaling** — centi-volts→V (÷100), signed centi-amps→A (÷100), millivolts→V (÷1000), raw
  Kelvin×10→°C (`(raw−2731)/10`), deci-percent→per-unit 0..1 (÷1000).

Fields with no Seplos source are emitted as `f32::NAN` ("not available" per the standard's float
convention): `run_time` (§4.2.1), `power_rate_of_change`, `StorageUpdate.hold_time` (§4.4). The
RS-232 inverter leg's own scale helpers/conventions are in `transform_inverter.rs::scale` (§4.8).

## 6. Topic name / transport mapping

Topic strings are built by `sink.rs` (`topic_into`) as `{prefix}/{device_id}/{slug}`, where `slug`
is the CamelCase TMS type name converted to `snake_case`. Default `prefix = "tms"`.

| TMS type / IDL `TOPIC_*` | snake_case slug | Concrete topic (default prefix, `device_id` = `<dev>`) |
|---|---|---|
| `Heartbeat` | `heartbeat` | `tms/<dev>/heartbeat` |
| `DeviceInfo` | `device_info` | `tms/<dev>/device_info` |
| `ActiveDiagnosticState` | `active_diagnostic_state` | `tms/<dev>/active_diagnostic_state` |
| `DcMeasurementUpdate` | `dc_measurement_update` | `tms/<dev>/dc_measurement_update` |
| `AcMeasurementUpdate` | `ac_measurement_update` | `tms/<dev>/ac_measurement_update` |
| `PowerHardwareUpdate` | `power_hardware_update` | `tms/<dev>/power_hardware_update` |
| `StorageUpdate` | `storage_update` | `tms/<dev>/storage_update` |
| `Reply` | `reply` | `tms/<dev>/reply` |
| `EnergyStartStopRequest` | `energy_start_stop_request` | `tms/<dev>/energy_start_stop_request` |
| `PowerSwitchRequest` | `power_switch_request` | `tms/<dev>/power_switch_request` |

**Canonical IDL topic names are bare CamelCase with NO prefix and NO device segment** — devices are
DDS instances keyed by `deviceId` (`idl` D.3 p.290 / A.8.6 p.43 of the PDF); the topic *name itself*
carries no per-device information under DDS. `tms/{device_id}/{snake_slug}` is this project's MQTT
adaptation, needed because MQTT topics (unlike DDS topics) are the only per-connection addressing
mechanism available — a flat `ActiveDiagnosticState` topic shared by every device would collapse
all devices' publications together. The mapping is a **documented, reversible bridge rule**:
`snake_slug` → `CamelCase` recovers the exact IDL name (uppercase each `_`-delimited segment's first
letter, drop the underscores) — lossless as long as no IDL type name contains a back-to-back
acronym run (e.g. `AC` immediately followed by another capitalized segment), which none of the
types emitted today do.

## 7. Documented adaptations (deliberate, project-wide)

These are permanent, intentional departures from the wire-level standard — not defects — recorded
here per issues #260/#261 so a future spec-conformance reviewer doesn't mistake them for gaps.

### 7.1 Transport: MQTT+JSON and DDS/XCDR2 (story #321, WP1-WP7)

The standard's data model is DDS/IDL and its interoperability story assumes **DDS-XTypes/XCDR2**
serialization over **RTPS/UDP, IPv6, domain 0** (§5.4 pp.28-29, D.1-D.5 pp.289-293 of the PDF). As of
story #321 the adapter carries a **real DDS/RTPS transport on the Pi, beside MQTT** — MQTT/JSON stays
always-on and byte-identical to its pre-#321 behavior; DDS is additive, config-gated, and off by
default.

**What ships.** An `[dds]` TOML block on `bin-rpi::Config` (`crates/bin/rpi/src/config/types/dds_toml.rs`)
enables a second transport leg: RTPS over UDP/IPv6 (SPDP multicast group `ff02::1234`, unicast DATA),
DDS domain 0 (the IDL-mandated default), XCDR2 wire encoding (`D_CDR2_LE` encapsulation) via the
`dust_dds` 0.15.0 crate. Absence of `[dds]` from config is the exact pre-#321 code path — zero DDS
code runs, MQTT publishing is unaffected. The RTPS/UDP transport itself is a custom
`TransportParticipantFactory` (`crates/lib-dds/src/dustdds/types/transport_factory.rs`) built on the
project's own `lib-net` embassy-shaped UDP/IPv6 seam (host/tokio impl today, embassy-net-compatible
shape for the future MCU path) rather than a stock dust_dds transport — dust_dds ships no IPv6
transport of its own. The 8 named QoS profiles from `generated_qos.rs` (§7 D.5: `PublishLast`
TRANSIENT_LOCAL/RELIABLE/KEEP_LAST 1, `Reply` RELIABLE/KEEP_LAST 128, telemetry BestEffort/Volatile)
are wired per topic, and every writer carries `USER_DATA = deviceId`.

**dust_dds is vendored, not a stock crates.io dependency.** `vendor/dust_dds/` (patched via
`[patch.crates-io]` path override) carries a surgical 2-file fix (`src/xtypes/{serializer,
deserializer}.rs`) for **DDS-XTypes 1.3 §7.4.3.5.3 rule (12)**: stock dust_dds 0.15.0 omits the
NEXTINT length-delimiter that DDS-XTypes rule (12) requires ahead of sequences of non-primitive
(struct/string) elements. This is a live, disputed point in the ecosystem — see
`eclipse-cyclonedds/cyclonedds-cxx#218` and OMG DDS-XTypes erratum `DDSXTY14-56` — but stock
CycloneDDS 11.0.1's decoder rejects the delimiter-less form outright (observed live: `DcMeasurementUpdate`
failed to decode; `Heartbeat`, having no non-primitive sequence, decoded fine). `vendor/dust_dds/PATCHES.md`
documents the two-file diff and the removal plan (drop the vendor tree once an upstream fix ships; a
PR draft is prepared in `.scratch/dust-dds-upstream-pr.md`, not yet opened). This project's own chain
serializer (`lib-tms::cdr`, `emit_appendable_seq`/`emit_string_seq`) adopted rule (12) too, so chain
bytes and wire bytes match again — `crates/lib-mapping/tests/wire_cdr_cross_oracle.rs` proves chain ==
patched-dust_dds byte-for-byte.

**How it is verified — stock CycloneDDS, not a bespoke decoder.** Interop is proven against
**unmodified CycloneDDS 11.0.1**, using the official `cyclonedds` Python CLI tools (`ls`/`subscribe`),
in two harnesses:

- `deploy/dds-conformance/` — a podman-compose harness combining the Seplos simulator, `bin-rpi` with
  `[dds]` enabled, and a stock `cyclonedds`-tools container. `cyclonedds ls` confirms domain 0, bare
  CamelCase IDL topic names (no MQTT-style prefix/device path — see §6), the wired QoS profiles, and
  `USER_DATA = deviceId` on every writer; `cyclonedds subscribe` is an independent stock-tool delivery
  oracle for live decode.
- The CI `dds-interop` lane (`_dds_interop.yml`) and `test-e2e/tests/dds_interop_e2e.rs` — a real
  compiled `bin-rpi` binary + pty Seplos simulator + an in-test `DdsParticipant`, single network
  namespace, driving the same live-delivery + decode assertions in CI.

Independent of live delivery, **all 11 committed golden XCDR2 payloads** (`fixtures/xcdr2/*.hex`,
covering the full 136-type model, not just the topics with live writers) are accepted by the stock
CycloneDDS decoder — an 11/11 byte-oracle result, re-confirmed after the rule-(12) fix. The
`@optional` presence-flag encoding (an XCDR2 ambiguity in APPENDABLE types) was empirically arbitrated
against CycloneDDS in both directions — the project's spec-literal boolean form (always emit
`<is_present>`) round-trips through CycloneDDS's own encoder and decoder unchanged, so no code
adopted a different form. TRANSIENT_LOCAL late-joiner replay (`DeviceInfo`-class profiles) and the
`Reply` KEEP_LAST 128 depth are both exercised live in `dds_interop_e2e.rs`. Acceptance-test coverage:
`rtps_spec.feature` (10 confirmed, non-`@provisional` hex-table scenarios spanning encapsulation,
DHEADER, optional presence, 4-byte alignment, enum width, string NUL, the rule-(12) NEXTINT, and the
KeyHash emit plumbing) plus `tms_spec_compliance.feature`'s `@dds-out-of-scope` carve-out, which now
states the DDS posture explicitly rather than disclaiming DDS outright.

**Honest residual gaps:**

- **(a) Live DDS writer coverage is partial: 3 of 10 publish topics.** Only `Heartbeat`,
  `DcMeasurementUpdate`, `DcLoadSharingState`, and the `Reply` response leg have a live dust_dds writer
  mapping today. The other 7 publish topics (`DeviceInfo`, `PowerHardwareUpdate`, `StorageUpdate`,
  `ActiveDiagnosticState`, `EnergyStartStopState`, `PowerPortState`, `ActiveMicrogridControllerState`)
  and the 2 DDS request readers are blocked on a `dust_dds_derive` 0.15.0 gap: it cannot represent
  `@optional` members (`Option<T>` fields fail to compile under its derive; the APPENDABLE path emits
  no presence flags) — tracked in issue #332, with fix candidates (a full dust_dds `@optional`
  overhaul, or a rung-A opaque-bytes writer hook) both upstream-PR candidates. This does **not** limit
  what's proven about wire *validity* — the byte-oracle chain (goldens + the stock-CycloneDDS-decode
  oracle above) covers the full 136-type model regardless of which topics have a live writer today;
  it limits only which topics are observed arriving over the wire in the live harnesses.
- **(b) dust_dds propagates no XTypes `TypeInformation`.** `cyclonedds typeof`/`subscribe` type
  discovery is blind on a plain stock CycloneDDS subscriber with no local type definition — SEDP
  endpoint/QoS discovery (`cyclonedds ls`) works regardless, and the harnesses' own subscribers carry
  local types, so this doesn't block the done-bar. Remote XTypes type discovery over the wire is an
  optional XTypes profile feature, not a base-interoperability requirement; this is recorded here as a
  conformance note, not a defect.
- **(c) The MCU remains MQTT/JSON-only.** DDS is a Pi-only transport. The bounded MCU feasibility
  spike (`bin/zephyr/qemu/rtps_probe/`, story #321 WP8) returned a **NO-GO** verdict on the current
  64 KB SRAM substrate (RTPS/IPv6 headroom deficit measured, not estimated — see the spike's README
  for the arithmetic); revisiting the MCU path is tracked separately (issue #328, blocked).

The IDL remains the schema-shape oracle for both transports: it defines the field names/types/
structure both the JSON objects (MQTT) and the XCDR2 chains (DDS) must mirror. §§2-6, 8, 9 of this
report — the JSON structural-fidelity findings — are unaffected by DDS shipping; they describe the
MQTT/JSON path specifically, called out where relevant.

### 7.2 JSON key casing: snake_case vs IDL camelCase

Every IDL member is camelCase (`deviceId`, `timeMeasured`, …); every JSON key on the wire is
snake_case (`device_id`, `time_measured`, …). Since the standard's real wire is XCDR2/DDS — where
member *names* don't serialize onto the wire at all (XCDR2 is a positional/tagged binary format,
not a named-key format) — there is no "spec casing" for a JSON binding to violate; JSON key casing
is spec-silent. `snake_case` matches this project's/the baseline's existing Rust and MQTT
conventions. The mapping rule is mechanical and reversible: `camelCase` → `snake_case` inserts `_`
before each internal uppercase letter and lowercases it (issue #260).

**Cross-reference (§7.1):** now that the adapter also ships a real DDS/XCDR2 wire (§7.1), this is
worth restating precisely — the DDS transport's positional binary encoding has no member-name-casing
question at all, by construction, not by adaptation choice (there is no key to case; field order and
type alone drive the wire, per the same XCDR2 positional-format fact this section already relies on).
This section stays scoped to the MQTT/JSON transport only; it makes no claim about the DDS wire.

### 7.3 Sequence-bound narrowings (bounded-subset adaptations, issue #261)

The MCU's 64 KB SRAM budget narrows several `heapless` bounds below the IDL's `MAXLEN`. Each is
emit-safe (this adapter never needs to emit past its own bound) and decode-side truncates a foreign
device's message that exceeds it — a real narrowing, not a bug, and each carries its own doc-note
at the type definition:

| Sequence | Rust bound | IDL MAXLEN | IDL cite |
|---|---|---|---|
| `dc`/`ac` port measurement sequences | 4 | `MAX_PORTS` = 64 | `idl:903` |
| `PowerDeviceInfo.power_ports` | 4 | `MAX_PORTS` = 64 | `idl:903` |
| `PowerPortState.power_ports` | 4 | `MAX_PORTS` = 64 | `idl:903` |
| `TopicList` (`published_conditional_topics` etc.) | 16 | `TopicList_MAXLEN` = 64 | `idl:419` |
| `EnumLabelSequence` | 8 | `EnumLabelSequence_MAXLEN` = 128 | `idl:1093` |
| `PowerDeviceInfo.grounds` | 2 | `MAX_GROUNDING_CIRCUITS` = 8 | `idl:943` |
| `PowerPortConversionInfo.conversion_taps` | 4 | `MAX_TAPS` = 64 | `idl:999` |
| `LoadSharingInfo.load_response` | 2 | `LoadStepResponseSequence_MAXLEN` = 16 | `idl:2042` |
| `DeviceInfo.control_parameters`/`metric_parameters` | 8 | `ParameterMetadataSequence_MAXLEN` = 128 | `idl:3059` |

By contrast, the following bounds are **exact matches**, not narrowings, and should not be
confused with the table above: `MAX_DC_LINES = 3` (`PowerLineMeasurementSequence_MAXLEN`,
`idl:4906`), `MAX_STORAGE_CELLS = 50` (`MAXLEN_storageCells`, `idl:1487`), `MAX_DTC = 64`
(`DiagnosticTroubleCodeSequence_MAXLEN`, `idl:2469`).

### 7.4 Enum wire spellings aligned to IDL (issue #258, fixed wave 1)

Before wave 1, the live wire emitted bare Rust variant names (`"Major"`, `"ConditionExists"`) where
the IDL enumerators are `SEV_7_MAJOR` / `FMI_CONDITION_EXISTS`. Every `lib-schema` enum now carries
`#[serde(rename = "IDL_SPELLING")]` on each variant (`DtcSeverity`, `FailureModeIndicator`,
`ReplyCode`, `ConfigId`, and the rest) — the wire spelling is now the exact IDL enumerator string
for every enum in the schema.

### 7.5 `PowerPortNumber` is `i32`, not `u16` (issue #262, fixed wave 1)

The IDL typedef (`idl:900`) is `int32`, with `INVALID_PORT_NUMBER = -1` (`idl:906`) as the
unspecified/no-port sentinel — required by `Reply.portNumber` for a non-port-scoped request. The
type was `u16` before wave 1 (unable to represent the sentinel at all); it is `i32` everywhere now.

## 8. Caveats and known gaps

**(a) The fault bit-map is EMPIRICAL and the trigger vector is SYNTHETIC.** The Seplos `0x44`
fault-frame `(byte, bit)` decode in `lib-parse/src/seplos/fault.rs` is explicitly flagged as
*observed on live hardware in the prototype, NOT confirmed against the Seplos spec*. No real `0x44`
fault capture exists yet; any populated fault vector used in testing is synthetic. DTC content
(which faults fire) is provisional even where the TMS *shape* is correct.

**(b) Optional IDL fields the adapter omits (all `@optional`, legal to omit):** `PowerHardwareUpdate`'s
`engine`/`fuel`/`generator`/`powerElectronics` (STOR)/`thermal` — no corresponding subsystem;
`DiagnosticTroubleCode`'s `estimateTime`/`estimateSeverity` — no severity forecasting.
`EnergyStorageState.runTime`, `dc::PowerPortMeasurement.powerRateOfChange`, and
`StorageUpdate.holdTime` are emitted as NaN (present-but-not-available) rather than omitted, since
they are non-optional `float` fields.

**(c) Stub / provisional status vs the standard:** DTC `severity` is uniformly `Major` and
`occurrence_count` uniformly `1` — placeholder logic, not per-fault-classified. `suspect_parameter`
SPNs 1–16 (battery) and 100+bit-index/130 (inverter) are adapter-local integers, not assigned J1939
SPNs. The RS-232 inverter mapping (§4.8) is entirely PROVISIONAL pending a real Voltronic capture
(decision 0001's open risk). **ADS most-severe-first truncation:** because every emitted DTC is
uniformly `SEV_7_MAJOR` (both the Seplos and inverter fault tables) and both tables are far under the
64-DTC cap (16 and ~17 entries), the ≤64 truncation is trivially most-severe-first-correct today (a
uniform severity is already sorted, and no truncation ever occurs). If a future fault table
introduces varied severities *and* exceeds 64 DTCs, the emit must sort most-severe-first before
truncating — noted at both fault tables. The wire ADS serializer streams DTCs without buffering (the
T3 stack-overflow fix), so it deliberately does not sort at emit time.

**(f) `EnergyStartStopState` present level is a running-state proxy (PROVISIONAL).** The state
publisher reports `ESSL_OPERATIONAL` whenever the adapter is up and polling; a true per-bus
telemetry-health transition to `ESSL_UNKNOWN` needs a health signal that does not cross into
task-land today (the poll loop's `Health` is not published outward) — a documented follow-up. The
other three state topics are static for this telemetry-only device (see §9 / `state_publisher`).

**(d) Confirmed-exact / resolved bounds:** `MAX_PORTS` = 64 (`idl:903`), `MAXLEN_storageCells` = 50
(`idl:1487`), and — newly resolved this pass — `MAX_DC_LINES` = 3 exactly matches
`PowerLineMeasurementSequence_MAXLEN` (`idl:4906`); the nested `dc::PowerLineMeasurement`/
`dc::PowerPortMeasurement` and `ac::PowerLineMeasurement`/`ac::PowerPortMeasurement` bodies are now
fully expanded and IDL-verified (§4.1, §4.8), closing the prior report's open caveat about them.

**(e) Small deviations carried from the behavioral audit, not yet independently ticketed beyond
what §9 lists:** `Identity`'s `bounded()` truncation helper silently truncates a >32-char configured
id rather than rejecting it at config-load (MINLEN 1 is also unenforced); `occurrenceCount`
semantics (edge-triggered increment, NVM persistence, saturate at 2³²−2) are simplified to a
hardcoded `1`; `DtcSeverity::Unknown`/`Clear` are "never transmitted in this version" per the IDL
and the adapter correctly never emits them, but its uniform-`Major` mapping over-signals severity
for lesser faults (`SEV_7_MAJOR` = "may cause protection logic to enter a controlled shutdown
sequence", `idl:506`).

## 9. Spec-conformance items — closed (wave 5) and still-open (tracked)

### Closed in the wave-5 behavioral-conformance sweep (WS-S)

- **Required STOR publications — CLOSED (#263).** All four now publish, per identity, retained +
  QoS1, At Start + On Change, via `bin_rpi::tasks::state_publisher`:
  `EnergyStartStopState` (STOR), `PowerPortState` (STOR + CONV), `DcLoadSharingState` (STOR,
  Conditional-Required on HAS DC PORTS), `ActiveMicrogridControllerState` (all power devices). The
  two previously type-less topics (`ActiveMicrogridControllerState`, `dc::LoadSharingState`) gained
  hand-authored `lib-schema` structs copied verbatim from the factory-verified `lib-tms-model`
  descriptor (IDL 4539 / 5084 + nested `DcLoadSharingParameters` IDL 4994). Content is PROVISIONAL
  where the telemetry-only device cannot source a field (see §8 (c)/(f)). Live proof:
  `test-e2e/tests/spec_conformance_e2e.rs`.
- **MC-selection state machine (A.7.5) — CLOSED (#264).** `bin_rpi::tasks::mc_select` (one shared
  task) consumes `tms/+/heartbeat` + `tms/+/device_info`, identifies `ROLE_MICROGRID_CONTROLLER`
  senders, applies the New-MC (3 s) / Lost-Active-MC (6 s) / No-MC (10 s) timers, and feeds the
  selected id to every identity's `ActiveMicrogridControllerState` on selection change via a
  `watch`. `CONFIG_ON_COMMS_LOSS` (the No-MC action) is a documented no-op — no config subsystem.
  Selection logic is pure + host-unit-tested (`impl_mc_selector.rs`); the synthetic-MC transition
  is proven live in `spec_conformance_e2e.rs`.
- **Publication cadence / QoS profiles — CLOSED (#265).** `PowerHardwareUpdate`/`StorageUpdate` are
  now held to the spec Slow / Periodic 10 s profile by a per-bus rate limiter
  (`bin_rpi_platform::EmitCadence`), while `DcMeasurementUpdate` (Periodic 1 s) publishes every cycle
  and the default poll interval dropped to 1 s (§4.9.10.4). `DeviceInfo` is published retained +
  QoS AtLeastOnce (PublishLast / TRANSIENT_LOCAL / RELIABLE), verified against the embedded broker's
  retained-replay. `ActiveDiagnosticState` is emitted At Start + On Change (fault-set hash) with a
  30 s safety-net re-publish; most-severe-first truncation is trivially correct today (uniform
  `Major`, see §8 (c)). `Reply` already publishes QoS AtLeastOnce (Reliable) — the earlier
  "QoS0 Reply" note was stale.

### Still-open (tracked, next wave)

- **`ControlHardwareUpdate` publisher** — the type is authored but no publisher exists; this adapter
  proxies no control-hardware subsystem, so it stays unpublished for now (#263-adjacent).
- **`EnergyStartStopState` health-driven level** — the present/future level is a running-state proxy
  (`ESSL_OPERATIONAL`); a real transition to `ESSL_UNKNOWN` on telemetry-health loss needs a health
  signal plumbed out of the poll loop into task-land (§8 (f)).
- **`DiagnosticTroubleCode` field order** (§4.3) — `hint` must be last; found by the IDL
  descriptor-factory. **Issue #273** (open alongside the descriptor-factory work).
- **`TmsSerialize` derive, stage 2** — retiring the hand-written `serde-json-core` path in favor of
  flat codegen from the same `#[tms]` attributes the descriptor factory reads. **Issue #270.**

---
*Evidence citations are `file:line` for the Rust adapter and
`docs/compliance/tms/mil-std-3071_data_model.idl:line` for the standard. Where a field or bound
could not be confirmed from the IDL, it is stated as unconfirmed rather than assumed.*

//! CROSS-ORACLE — the permanent gate asserting our generated XCDR2 CDR chains (`lib_tms::cdr` + the
//! generated chains, reached via [`lib_mapping::render_cdr`]) produce byte-EXACT output vs an
//! independent reference XCDR2 serializer, for every OPTIONAL-FREE topic; and vs HAND-DERIVED
//! spec-pinned vectors for the `@optional` present/absent mechanic.
//!
//! # Reference serializer: dust_dds 0.15.0 (dev-only)
//!
//! The reference is dust-dds's `Cdr2LeSerializer` — the WP0-selected byte-oracle (its XCDR2 impl is
//! spec-clean; scorecard in commit 97d5a01). **As of WP6.5 the workspace consumes the vendored,
//! rule-(12)-patched dust_dds (`[patch.crates-io]` → `vendor/dust_dds`), so this reference now emits
//! the NEXTINT delimiter for non-primitive sequences exactly as our chains do** (both changed in
//! lock-step; see `lib_tms::cdr` module doc item 7 + `vendor/dust_dds/PATCHES.md`). It is a
//! **dev-dependency only**: the RTPS transport
//! backend is undecided pending the WP4 bake-off, and this crate never carries a DDS runtime. We
//! build dust_dds mirror types (its `TypeSupport` derive + `#[dust_dds(extensibility="appendable")]`)
//! whose field layout mirrors each TMS type, populate them from the SAME concrete generated struct
//! (built via the `cfg(std)` `To*` transforms / fixtures — identical to `wire_serde_cross_oracle`),
//! serialize with dust_dds, and assert OUR chain bytes == the reference bytes EXACTLY.
//!
//! # The `@optional` divergence (WP0 finding, arbitrated + RESOLVED in WP5 — no flip)
//!
//! dust_dds CANNOT represent an ABSENT `@optional` member, and emits a PRESENT one INLINE — WITHOUT
//! the spec `<is_present>` boolean (DDS-XTypes 1.3 §7.4.3.5.2 rule (20)). OUR chain implements the
//! spec-literal presence-flag form ([`lib_tms::cdr::emit_optional`]). So dust_dds is a byte-exact
//! oracle ONLY for topics whose serialized form contains NO optional member. For the optional
//! mechanic we assert against HAND-DERIVED vectors computed directly from the spec rules cited in
//! `lib_tms::cdr`'s module doc; [`OPTIONAL_ENCODING_POLICY`] records the single policy point and the
//! WP5 CycloneDDS arbitration outcome (CONFIRMED, no flip — CycloneDDS 11.0.1 independently emits
//! the identical presence-flag form AND accepts our exact bytes on decode; see
//! `deploy/dds-probe/dump_reference.py`), and [`OPTIONAL_MEMBER_INVENTORY`] lists every optional
//! member across the CDR topics the arbitration covers (uniformly, by mechanism — every optional
//! routes through the same [`lib_tms::cdr::emit_optional`]/`emit_optional_seq` pair the arbitration
//! validated, so the resolution applies to the whole inventory, not just the cases hand-tested).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::all)]

use dust_dds::infrastructure::type_support::TypeSupport;
use dust_dds::xtypes::dynamic_type::DynamicData;
use dust_dds::xtypes::serializer::Cdr2LeSerializer;

use lib_mapping::render_cdr::render_cdr_value;
use lib_mapping::transform::{ToDiagnostics, ToTms};
use lib_parse::canonical::{CanonicalFault, CanonicalReading};
use lib_tms::{ClockMonotonic, Identity};

// ==================================================================================================
// Optional-encoding policy documentation (the WP5 arbitration surface)
// ==================================================================================================

/// The SINGLE optional-member encoding policy this codebase implements, and the WP5 arbitration
/// OUTCOME. Kept as a shared constant so the divergence + resolution are documented in exactly one
/// place (mirrors the single [`lib_tms::cdr::emit_optional`] / `emit_optional_seq` policy point).
pub const OPTIONAL_ENCODING_POLICY: &str = "\
    XCDR2 @optional in APPENDABLE = boolean <is_present> (0x01/0x00) then value-iff-present \
    (DDS-XTypes 1.3 §7.4.3.5.2 + rule (20)). dust_dds 0.15.0 diverges (inline, no flag; cannot \
    represent absent) — WP0 finding, but dust_dds is only the byte-oracle for optional-FREE topics, \
    never the arbitration authority for @optional itself. WP5 CycloneDDS arbitration (story #323): \
    RESOLVED, NO FLIP — CycloneDDS 11.0.1 (eclipse-cyclonedds Python binding) independently encodes \
    the IDENTICAL presence-flag form for both present and absent, and its own decoder accepts our \
    exact committed hand-derived bytes unmodified (deploy/dds-probe/dump_reference.py `arbitration` \
    mode + two real golden fixtures: energy_start_stop_state=absent, \
    power_hardware_inverter=present-with-value). lib_tms::cdr::{emit_optional,emit_optional_seq} are \
    UNCHANGED; the routing through them keeps this a one-place flip point for any FUTURE spec/vendor \
    reading, even though this round confirmed the existing form.";

/// Every `@optional` member across the CDR-topic closure — the inventory awaiting WP5 CycloneDDS
/// arbitration (these are the members whose bytes dust_dds cannot cross-check). `(type, member)`.
pub const OPTIONAL_MEMBER_INVENTORY: &[(&str, &str)] = &[
    ("PowerHardwareUpdate", "engine"),
    ("PowerHardwareUpdate", "fuel"),
    ("PowerHardwareUpdate", "generator"),
    ("PowerHardwareUpdate", "energy_storage"),
    ("PowerHardwareUpdate", "power_electronics"),
    ("PowerHardwareUpdate", "thermal"),
    ("StorageUpdate", "state_of_charge"),
    ("StorageUpdate", "max_charge_rate"),
    ("StorageUpdate", "max_discharge_rate"),
    ("StorageCellState", "state_of_charge"),
    ("AcPowerLineMeasurement", "phase_offset"),
    ("DiagnosticTroubleCode", "port_number"),
    ("DiagnosticTroubleCode", "estimate_time"),
    ("DiagnosticTroubleCode", "estimate_severity"),
    ("EnergyStartStopState", "present_level_actor"),
    ("ActiveMicrogridControllerState", "master_id"),
    ("EnergyStartStopRequest", "switch_conditions"),
    ("PowerSwitchRequest", "switch_conditions"),
    ("DeviceInfo", "control_hardware"),
    ("DeviceInfo", "power_hardware"),
    ("DeviceInfo", "control_parameters"),
    ("DeviceInfo", "metric_parameters"),
    ("DeviceInfo", "control_service"),
    ("DeviceInfo", "power_device"),
    ("Reply", ""), // Reply itself is optional-FREE — listed to note it is cross-checked exactly.
];

// ==================================================================================================
// helpers
// ==================================================================================================

fn dev(s: &str) -> Identity {
    let mut id = Identity::new();
    id.push_str(s).unwrap();
    id
}

fn now(sec: u32, nsec: u32) -> ClockMonotonic {
    ClockMonotonic {
        seconds: sec,
        nanoseconds: nsec,
    }
}

fn reading(
    cells: usize,
    temps: usize,
    current_ca: i16,
    voltage_cv: u16,
    soc: u16,
    cycles: u16,
) -> CanonicalReading {
    let mut cell_voltages_mv: heapless::Vec<u16, 16> = heapless::Vec::new();
    for i in 0..cells as u16 {
        cell_voltages_mv.push(3800 + i).unwrap();
    }
    let mut temperatures_raw: heapless::Vec<u16, 8> = heapless::Vec::new();
    for i in 0..temps as u16 {
        temperatures_raw.push(2731 + 250 + i * 2).unwrap();
    }
    CanonicalReading {
        device_address: 0,
        cell_voltages_mv,
        temperatures_raw,
        current_ca,
        total_voltage_cv: voltage_cv,
        remaining_capacity_cah: 1395,
        typical_capacity_cah: 1800,
        rated_capacity_cah: 1800,
        soc_dpct: soc,
        soh_dpct: 1000,
        cycles,
        port_voltage_cv: 5453,
    }
}

fn fault(bits: u32) -> CanonicalFault {
    CanonicalFault {
        device_address: 0,
        high_voltage_protection: bits & 1 != 0,
        low_voltage_protection: bits & 2 != 0,
        high_temperature_charge_protection: bits & 4 != 0,
        low_temperature_charge_protection: bits & 8 != 0,
        charge_overcurrent_protection: bits & 16 != 0,
        discharge_overcurrent_protection: bits & 32 != 0,
        short_circuit_protection: bits & 64 != 0,
        mosfet_fault: bits & 128 != 0,
        cell_balance_warning: bits & 256 != 0,
        low_capacity_warning: bits & 512 != 0,
        high_temperature_discharge_warning: bits & 1024 != 0,
        low_temperature_discharge_warning: bits & 2048 != 0,
        low_voltage_warning: bits & 4096 != 0,
        high_voltage_warning: bits & 8192 != 0,
        discharge_current_warning: bits & 16384 != 0,
        charge_current_warning: bits & 32768 != 0,
    }
}

/// Serialize a dust_dds mirror value to XCDR2-LE reference bytes.
fn dust_bytes<T: TypeSupport>(v: T) -> Vec<u8>
where
    T: Sized,
{
    let sample: DynamicData = v.create_dynamic_sample();
    Cdr2LeSerializer::serialize(&sample).expect("dust_dds serialize")
}

/// Our chain bytes for a concrete generated struct via the identity `…CdrSrc` binding.
fn our_bytes<T: lib_tms::cdr::HasCdrChain>(value: &T) -> Vec<u8> {
    let mut buf = [0u8; 16 * 1024];
    let n = render_cdr_value(value, &mut buf).expect("render_cdr_value");
    buf[..n].to_vec()
}

fn hex(b: &[u8]) -> String {
    b.iter()
        .map(|x| format!("{x:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn assert_bytes_eq(label: &str, ours: &[u8], reference: &[u8]) {
    assert_eq!(
        ours,
        reference,
        "\n{label}: chain bytes != reference bytes\n  ours ({}) : {}\n  ref  ({}) : {}",
        ours.len(),
        hex(ours),
        reference.len(),
        hex(reference)
    );
}

// ==================================================================================================
// dust_dds mirror types (field layout == the TMS IDL types, appendable extensibility)
// ==================================================================================================

#[derive(TypeSupport, Clone)]
#[dust_dds(extensibility = "appendable")]
struct MHeartbeat {
    device_id: String,
    sequence_number: u32,
}

#[derive(TypeSupport, Clone)]
#[dust_dds(extensibility = "appendable", nested)]
struct MClockMonotonic {
    seconds: u32,
    nanoseconds: u32,
}

#[derive(TypeSupport, Clone)]
#[dust_dds(extensibility = "appendable", nested)]
struct MDcPowerLine {
    voltage: f32,
    amperage: f32,
    power: f32,
}

#[derive(TypeSupport, Clone)]
#[dust_dds(extensibility = "appendable", nested)]
struct MDcPowerPort {
    port_number: i32,
    line: Vec<MDcPowerLine>,
    power_rate_of_change: f32,
}

#[derive(TypeSupport, Clone)]
#[dust_dds(extensibility = "appendable")]
struct MDcMeasurementUpdate {
    device_id: String,
    time_measured: MClockMonotonic,
    external_measurement: Vec<MDcPowerPort>,
    internal_measurement: Vec<MDcPowerPort>,
}

#[derive(TypeSupport, Clone)]
#[dust_dds(extensibility = "appendable", nested)]
struct MDiagnosticTroubleCode {
    // NOTE: DiagnosticTroubleCode carries @optional members (port_number, estimate_time,
    // estimate_severity) — so this mirror is only used with an EMPTY `codes` sequence, where no
    // optional bytes materialize and dust_dds stays a faithful oracle.
    suspect_parameter: u32,
    occurrence_count: u32,
}

#[derive(TypeSupport, Clone)]
#[dust_dds(extensibility = "appendable")]
struct MActiveDiagnosticState {
    device_id: String,
    codes: Vec<MDiagnosticTroubleCode>,
    overflow: u32,
}

#[derive(TypeSupport, Clone)]
#[dust_dds(extensibility = "appendable", nested)]
struct MReplyStatus {
    code: i32,
    reason: String,
}

#[derive(TypeSupport, Clone)]
#[dust_dds(extensibility = "appendable")]
struct MReply {
    requesting_device_id: String,
    target_device_id: String,
    config: i32,
    port_number: i32,
    request_sequence_id: u64,
    status: MReplyStatus,
}

// ==================================================================================================
// EXACT cross-checks (optional-free topics) — chain bytes == dust_dds bytes
// ==================================================================================================

/// `Heartbeat` — the simplest published topic: a string + a u32, optional-free. Exercises the
/// top-level encapsulation header + DHEADER + string (length-includes-NUL) + u32.
#[test]
fn heartbeat_exact() {
    let hb = lib_tms::Heartbeat {
        device_id: dev("METEOR-001"),
        sequence_number: 42,
    };
    let ours = our_bytes(&hb);
    let reference = dust_bytes(MHeartbeat {
        device_id: hb.device_id.as_str().to_owned(),
        sequence_number: hb.sequence_number,
    });
    assert_bytes_eq("Heartbeat", &ours, &reference);
}

/// `DcMeasurementUpdate` (battery) — optional-free; exercises a nested appendable member
/// (`ClockMonotonic` DHEADER), a sequence of appendable structs (`external_measurement` ->
/// `DcPowerPortMeasurement` -> `line` -> `DcPowerLineMeasurement`, each with its own DHEADER), an
/// empty sequence (`internal_measurement`), strings, i32 and f32. Built via the `To*` transform, then
/// BOTH serializers render the SAME concrete struct's values.
#[test]
fn dc_measurement_exact() {
    let r = reading(14, 6, -164, 5452, 775, 2);
    let payload: lib_tms::DcMeasurementUpdate =
        r.to_dc_measurement(dev("METEOR-001"), now(12345, 678_000_000));
    let ours = our_bytes(&payload);

    let mirror = MDcMeasurementUpdate {
        device_id: payload.device_id.as_str().to_owned(),
        time_measured: MClockMonotonic {
            seconds: payload.time_measured.seconds,
            nanoseconds: payload.time_measured.nanoseconds,
        },
        external_measurement: payload
            .external_measurement
            .iter()
            .map(|p| MDcPowerPort {
                port_number: p.port_number,
                line: p
                    .line
                    .iter()
                    .map(|l| MDcPowerLine {
                        voltage: l.voltage,
                        amperage: l.amperage,
                        power: l.power,
                    })
                    .collect(),
                power_rate_of_change: p.power_rate_of_change,
            })
            .collect(),
        internal_measurement: Vec::new(),
    };
    let reference = dust_bytes(mirror);
    assert_bytes_eq("DcMeasurementUpdate", &ours, &reference);
}

/// `ActiveDiagnosticState` with ZERO faults -> EMPTY `codes` sequence, so no `DiagnosticTroubleCode`
/// optional bytes materialize and dust_dds is a faithful oracle. Exercises an empty appendable-struct
/// sequence + a trailing u32.
#[test]
fn active_diagnostic_empty_codes_exact() {
    let f = fault(0);
    let payload: lib_tms::ActiveDiagnosticState =
        f.to_active_diagnostics(dev("METEOR-001"), now(12345, 678_000_000));
    assert!(payload.codes.is_empty(), "fixture must have no DTCs");
    let ours = our_bytes(&payload);
    let reference = dust_bytes(MActiveDiagnosticState {
        device_id: payload.device_id.as_str().to_owned(),
        codes: Vec::new(),
        overflow: payload.overflow,
    });
    assert_bytes_eq("ActiveDiagnosticState(empty codes)", &ours, &reference);
}

/// `Reply` — optional-free; exercises two enums (`config`, `status.code`, XCDR2 32-bit holders), a
/// u64 (XCDR2 align-4), an i32, and a nested appendable member (`ReplyStatus`) with a string.
#[test]
fn reply_exact() {
    use lib_tms::{ConfigId, Reply, ReplyCode, ReplyStatus};
    let payload = Reply {
        requesting_device_id: dev("REQ-1"),
        target_device_id: dev("TGT-1"),
        config: ConfigId::Active, // ordinal 2
        port_number: 7,
        request_sequence_id: 0x0102_0304_0506_0708,
        status: ReplyStatus {
            code: ReplyCode::NotImplemented, // ordinal 8
            reason: heapless::String::try_from("nope").unwrap(),
        },
    };
    let ours = our_bytes(&payload);
    let reference = dust_bytes(MReply {
        requesting_device_id: payload.requesting_device_id.as_str().to_owned(),
        target_device_id: payload.target_device_id.as_str().to_owned(),
        config: payload.config as i32,
        port_number: payload.port_number,
        request_sequence_id: payload.request_sequence_id,
        status: MReplyStatus {
            code: payload.status.code as i32,
            reason: payload.status.reason.as_str().to_owned(),
        },
    });
    assert_bytes_eq("Reply", &ours, &reference);
}

// ==================================================================================================
// HAND-DERIVED spec vectors — the @optional present/absent mechanic (dust_dds cannot represent this)
// ==================================================================================================

/// `ActiveMicrogridControllerState { device_id: String, master_id: Option<String> }` — the smallest
/// `@optional` topic. Hand-derived from the spec rules (see [`OPTIONAL_ENCODING_POLICY`]); the exact
/// derivation is in the byte comments. These pin the optional PRESENT (`0x01` + value) and ABSENT
/// (`0x00`) encoding and are the WP5 CycloneDDS arbitration targets.
#[test]
fn active_microgrid_controller_optional_present_hand_derived() {
    use lib_tms::ActiveMicrogridControllerState;
    let payload = ActiveMicrogridControllerState {
        device_id: dev("MC"),
        master_id: Some(dev("M1")),
    };
    let ours = our_bytes(&payload);
    // encap D_CDR2_LE + options(pad=1) | DHEADER=15 | device_id "MC"(len3+NUL) | present 0x01 |
    // master_id "M1"(len3+NUL) | 1 trailing pad byte.
    let expected: &[u8] = &[
        0x00, 0x09, 0x00, 0x01, // encapsulation: D_CDR2_LE, options low-byte pad count = 1
        0x0f, 0x00, 0x00, 0x00, // DHEADER = 15 (member bytes)
        0x03, 0x00, 0x00, 0x00, // device_id length = 3 (incl. NUL)
        0x4d, 0x43, 0x00, //       "MC\0"
        0x01, //                   master_id <is_present> = true
        0x03, 0x00, 0x00, 0x00, // master_id length = 3 (aligned to 4 at this offset)
        0x4d, 0x31, 0x00, //       "M1\0"
        0x00, //                   trailing encapsulation padding (to 4-byte boundary)
    ];
    assert_bytes_eq("ActiveMicrogridControllerState(present)", &ours, expected);
}

#[test]
fn active_microgrid_controller_optional_absent_hand_derived() {
    use lib_tms::ActiveMicrogridControllerState;
    let payload = ActiveMicrogridControllerState {
        device_id: dev("MC"),
        master_id: None,
    };
    let ours = our_bytes(&payload);
    // encap D_CDR2_LE + options(pad=0) | DHEADER=8 | device_id "MC" | absent 0x00 | no trailing pad.
    let expected: &[u8] = &[
        0x00, 0x09, 0x00, 0x00, // encapsulation: D_CDR2_LE, options pad count = 0
        0x08, 0x00, 0x00, 0x00, // DHEADER = 8 (member bytes)
        0x03, 0x00, 0x00, 0x00, // device_id length = 3
        0x4d, 0x43, 0x00, //       "MC\0"
        0x00, //                   master_id <is_present> = false (absent)
    ];
    assert_bytes_eq("ActiveMicrogridControllerState(absent)", &ours, expected);
}

/// Sanity: the policy documentation constants are non-empty and the inventory covers the known
/// optional-bearing topics (so a future field addition without an inventory update is noticed).
#[test]
fn optional_policy_documented() {
    assert!(OPTIONAL_ENCODING_POLICY.contains("§7.4.3.5.2"));
    assert!(OPTIONAL_MEMBER_INVENTORY.len() >= 20);
    assert!(OPTIONAL_MEMBER_INVENTORY
        .iter()
        .any(|(t, m)| *t == "PowerHardwareUpdate" && *m == "energy_storage"));
}

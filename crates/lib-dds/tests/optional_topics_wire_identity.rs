//! BYTE-IDENTITY + ROUND-TRIP gate for the 7 `@optional`-bearing published topics, over the
//! **vendored `@optional`-overhauled `dust_dds`** (#332). This is the optional-topic counterpart to
//! `crates/lib-mapping/tests/wire_cdr_cross_oracle.rs` (which cross-checks the optional-FREE topics
//! and pins the optional mechanic against hand-derived vectors): here we drive the SAME two rungs the
//! WP3 oracle uses, but now against real `dust_dds` patch types whose XCDR2 path emits the spec
//! presence flag.
//!
//! For each topic we assert TWO things, mirroring the WP3 cross-oracle, for BOTH an all-present and
//! an all-absent optional case (so the `0x01`+value present byte AND the `0x00` absent byte — the
//! wire distinction that was impossible before #332 — are both exercised):
//!
//! 1. **BYTE-IDENTITY (the cross-oracle).** `dust_dds` XCDR2-LE serialization of a `patch::T`
//!    `DynamicData` (via `TypeSupport::create_dynamic_sample` → `Cdr2LeSerializer::serialize`, the
//!    same reference path the DataWriter takes) equals OUR hand-written `lib_tms` CDR-chain bytes for
//!    the SAME value. The `patch::T` value is derived MECHANICALLY from the `lib_tms` value (the
//!    `to_patch_*` converters below) so "the same value" is structural, not eyeballed. This proves
//!    the vendored serializer emits the 1-byte `<is_present>` boolean in declared order exactly like
//!    `lib_tms::cdr::emit_optional` (present → `0x01`+value, absent → `0x00`, no preceding pad).
//!
//! 2. **ROUND-TRIP (rung C — the write-path decode).** `CdrDeserializer::deserialize(T::get_type(),
//!    bytes)` + `T::create_sample` reconstitutes those exact wire bytes back into a `patch::T` equal
//!    to the original — absent optionals → `None`, present → `Some(v)`. This is the vendored
//!    deserializer's overridable presence-consume (default no-op for CDR1/PL_CDR) doing its job on
//!    OUR wire form, i.e. the `DustWriter::write` pipeline (impl_dust_writer.rs) end to end.
//!
//! Coverage of the presence-byte placement the spec cares about (DDS-XTypes 1.3 §7.4.3.5.2 + rule
//! (20) §7.4.3.5.3):
//!   * `DeviceInfo` all-present + all-absent — the deepest nested closure; the present case sets the
//!     6 top-level optionals `Some(..)` whose OWN sub-optionals are absent, so `0x01` presence bytes
//!     at depth 1 wrap a cascade of `0x00` bytes at depth 2+ (mixed depth).
//!   * `PowerPortState` → `PowerPortStatus` → `PowerSwitchStatus` — a MID-struct optional
//!     (`present_continuity_actor`) followed by a REQUIRED member (`future_continuity_reason`),
//!     proving the non-trailing `0x00` byte is emitted (not elided as an absent trailer).
//!   * `DiagnosticTroubleCode` — a LEADING optional (`port_number` is member 0), the other
//!     non-trailing-`0x00` shape.
//!   * `EngineState` (inside `PowerHardwareUpdate`) — two NON-ADJACENT optionals straddling a
//!     required member (`wet_stack` … `run_time` … `fuel_consumption_rate`).
//!
//! No live CycloneDDS golden byte-vector for `DeviceInfo` is checked in under
//! `deploy/dds-conformance/` (only the Heartbeat/DcMeasurementUpdate text artifacts exist), so the
//! optional #323 golden-decode assertion is env-gated + skips by default; point
//! `SENTIENT_DDS_GOLDEN_DEVICEINFO` at a whitespace-separated hex dump to run it.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::all)]

use std::fmt::Debug;

use dust_dds::infrastructure::type_support::TypeSupport;
use dust_dds::xtypes::deserializer::CdrDeserializer;
use dust_dds::xtypes::dynamic_type::DynamicData;
use dust_dds::xtypes::serializer::Cdr2LeSerializer;

use lib_dds::dustdds::types::patch;
use lib_tms::cdr::{emit_object_cdr, CdrWriteSink, HasCdrChain};

// ==================================================================================================
// helpers
// ==================================================================================================

/// Build a `heapless::String<N>` from a `&str` (test fixtures only).
fn hstr<const N: usize>(s: &str) -> heapless::String<N> {
    heapless::String::try_from(s).expect("string fixture within capacity")
}

/// heapless `String<N>` → std `String` (patch types are std).
fn cs<const N: usize>(v: &heapless::String<N>) -> String {
    v.as_str().to_owned()
}

/// `Option<heapless::String<N>>` → `Option<String>`.
fn cos<const N: usize>(v: &Option<heapless::String<N>>) -> Option<String> {
    v.as_ref().map(|s| s.as_str().to_owned())
}

/// Build a bounded `heapless::Vec` from an iterator (panics on capacity overflow — fixtures are
/// deliberately small).
fn hvec<T, const N: usize>(items: impl IntoIterator<Item = T>) -> heapless::Vec<T, N> {
    let mut v = heapless::Vec::new();
    for it in items {
        assert!(v.push(it).is_ok(), "fixture exceeds heapless::Vec capacity");
    }
    v
}

/// OUR `lib_tms` CDR-chain bytes for a concrete generated struct — the full XCDR2 SerializedPayload
/// (encapsulation header + DHEADER + members + trailing pad), byte-for-byte what
/// `lib_mapping::render_cdr` / the bytes-in seam emits. Same construction as `dds_loopback::render`.
fn our_bytes<T: HasCdrChain>(v: &T) -> Vec<u8> {
    let mut buf = [0u8; 32 * 1024];
    let mut sink = CdrWriteSink::new(&mut buf);
    emit_object_cdr::<T, T::CdrChain, _>(v, &mut sink);
    sink.finalize_encapsulation_padding();
    assert!(!sink.overflowed(), "our_bytes render buffer overflow");
    let n = sink.len();
    buf[..n].to_vec()
}

/// The `dust_dds` reference XCDR2-LE bytes for a patch value — `create_dynamic_sample` →
/// `Cdr2LeSerializer::serialize`, the exact path the typed `DataWriter` re-serializes through.
fn dust_bytes<T: TypeSupport>(v: T) -> Vec<u8> {
    let sample: DynamicData = v.create_dynamic_sample();
    Cdr2LeSerializer::serialize(&sample).expect("dust_dds serialize")
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
        "\n{label}: lib_tms chain bytes != dust_dds patch bytes\n  ours ({}) : {}\n  ref  ({}) : {}",
        ours.len(),
        hex(ours),
        reference.len(),
        hex(reference)
    );
}

/// Rung C: deserialize `bytes` (OUR wire form) via the vendored `CdrDeserializer` against the patch
/// type's `DynamicType`, reconstitute a `patch::T` with `create_sample`, and assert it equals the
/// original — absent optionals → `None`, present → `Some(v)`.
fn assert_roundtrip<T>(label: &str, expected: &T, bytes: &[u8])
where
    T: TypeSupport + PartialEq + Debug,
{
    let dynamic = CdrDeserializer::deserialize(T::get_type(), bytes)
        .unwrap_or_else(|e| panic!("{label}: deserialize our wire bytes: {e:?}"));
    let sample = T::create_sample(dynamic);
    assert_eq!(
        &sample, expected,
        "{label}: rung-C round-trip mismatch (absent→None / present→Some)"
    );
}

/// Run BOTH rungs for one case: build OUR chain bytes from `lib_val`, the reference bytes from the
/// mechanically-derived `patch_val`, assert byte-identity, then round-trip OUR bytes back to a patch
/// sample and assert it equals `patch_val`.
fn check<L, P>(label: &str, lib_val: &L, patch_val: P)
where
    L: HasCdrChain,
    P: TypeSupport + Clone + PartialEq + Debug,
{
    let ours = our_bytes(lib_val);
    let reference = dust_bytes(patch_val.clone());
    assert_bytes_eq(label, &ours, &reference);
    assert_roundtrip(label, &patch_val, &ours);
}

// ==================================================================================================
// enum converters — lib_tms → patch (only the variants the fixtures use; `_` is unreachable at
// runtime because `check` only ever feeds the values constructed below)
// ==================================================================================================

fn p_role(r: lib_tms::DeviceRole) -> patch::DeviceRole {
    match r {
        lib_tms::DeviceRole::Source => patch::DeviceRole::Source,
        other => unreachable!("unmapped DeviceRole {other:?}"),
    }
}
fn p_cable(c: lib_tms::CableSenseStatus) -> patch::CableSenseStatus {
    match c {
        lib_tms::CableSenseStatus::Connected => patch::CableSenseStatus::Connected,
        other => unreachable!("unmapped CableSenseStatus {other:?}"),
    }
}
fn p_cont(c: lib_tms::CircuitContinuity) -> patch::CircuitContinuity {
    match c {
        lib_tms::CircuitContinuity::Closed => patch::CircuitContinuity::Closed,
        lib_tms::CircuitContinuity::Open => patch::CircuitContinuity::Open,
        other => unreachable!("unmapped CircuitContinuity {other:?}"),
    }
}
fn p_psr(r: lib_tms::PowerSwitchReason) -> patch::PowerSwitchReason {
    match r {
        lib_tms::PowerSwitchReason::Manual => patch::PowerSwitchReason::Manual,
        other => unreachable!("unmapped PowerSwitchReason {other:?}"),
    }
}
fn p_pstc(c: lib_tms::PowerSwitchTransitionCode) -> patch::PowerSwitchTransitionCode {
    match c {
        lib_tms::PowerSwitchTransitionCode::Complete => patch::PowerSwitchTransitionCode::Complete,
        other => unreachable!("unmapped PowerSwitchTransitionCode {other:?}"),
    }
}
fn p_fmi(f: lib_tms::FailureModeIndicator) -> patch::FailureModeIndicator {
    match f {
        lib_tms::FailureModeIndicator::VoltageHigh => patch::FailureModeIndicator::VoltageHigh,
        other => unreachable!("unmapped FailureModeIndicator {other:?}"),
    }
}
fn p_sev(s: lib_tms::DtcSeverity) -> patch::DtcSeverity {
    match s {
        lib_tms::DtcSeverity::Warning => patch::DtcSeverity::Warning,
        lib_tms::DtcSeverity::Critical => patch::DtcSeverity::Critical,
        other => unreachable!("unmapped DtcSeverity {other:?}"),
    }
}
fn p_eos(s: lib_tms::EngineOperatingState) -> patch::EngineOperatingState {
    match s {
        lib_tms::EngineOperatingState::Running => patch::EngineOperatingState::Running,
        other => unreachable!("unmapped EngineOperatingState {other:?}"),
    }
}
fn p_essl(l: lib_tms::EnergyStartStopLevel) -> patch::EnergyStartStopLevel {
    match l {
        lib_tms::EnergyStartStopLevel::Operational => patch::EnergyStartStopLevel::Operational,
        lib_tms::EnergyStartStopLevel::Ready => patch::EnergyStartStopLevel::Ready,
        other => unreachable!("unmapped EnergyStartStopLevel {other:?}"),
    }
}

// ==================================================================================================
// struct converters — lib_tms → patch (field-for-field; the "same value" guarantee for rung 1)
// ==================================================================================================

fn p_clock(c: &lib_tms::ClockMonotonic) -> patch::ClockMonotonic {
    patch::ClockMonotonic {
        seconds: c.seconds,
        nanoseconds: c.nanoseconds,
    }
}
fn p_point(p: &lib_tms::Point2D) -> patch::Point2D {
    patch::Point2D { x: p.x, y: p.y }
}
fn p_curve(c: &lib_tms::Curve2D) -> patch::Curve2D {
    patch::Curve2D {
        points: c.points.iter().map(p_point).collect(),
    }
}
fn p_engine(e: &lib_tms::EngineState) -> patch::EngineState {
    patch::EngineState {
        operating_state: p_eos(e.operating_state),
        oil_pressure: e.oil_pressure,
        coolant_temperature: e.coolant_temperature,
        speed: e.speed,
        wet_stack: e.wet_stack,
        run_time: e.run_time,
        fuel_consumption_rate: e.fuel_consumption_rate,
    }
}
fn p_fuel(f: &lib_tms::FuelState) -> patch::FuelState {
    patch::FuelState {
        fuel_level: f.fuel_level,
        fuel_pump_running: f.fuel_pump_running,
    }
}
fn p_gen(g: &lib_tms::GeneratorState) -> patch::GeneratorState {
    patch::GeneratorState {
        field_current: g.field_current,
        stator_temperature: g.stator_temperature,
    }
}
fn p_cell(c: &lib_tms::StorageCellState) -> patch::StorageCellState {
    patch::StorageCellState {
        cell_number: c.cell_number,
        cycle_counter: c.cycle_counter,
        state_of_charge: c.state_of_charge,
        temperature: c.temperature,
        voltage: c.voltage,
    }
}
fn p_ess(e: &lib_tms::EnergyStorageState) -> patch::EnergyStorageState {
    patch::EnergyStorageState {
        min_temperature: e.min_temperature,
        average_temperature: e.average_temperature,
        max_temperature: e.max_temperature,
        run_time: e.run_time,
        storage_cells: e.storage_cells.iter().map(p_cell).collect(),
    }
}
fn p_pe(p: &lib_tms::PowerElectronicsState) -> patch::PowerElectronicsState {
    patch::PowerElectronicsState {
        temperature: p.temperature,
    }
}
fn p_thermal(t: &lib_tms::ThermalState) -> patch::ThermalState {
    patch::ThermalState {
        thermal_load: t.thermal_load.iter().copied().collect(),
    }
}
fn p_portid(p: &lib_tms::PowerPortId) -> patch::PowerPortId {
    patch::PowerPortId {
        device_id: cs(&p.device_id),
        port_number: p.port_number,
    }
}
fn p_switch(s: &lib_tms::PowerSwitchStatus) -> patch::PowerSwitchStatus {
    patch::PowerSwitchStatus {
        present_continuity: p_cont(s.present_continuity),
        future_continuity: p_cont(s.future_continuity),
        request_lock: s.request_lock,
        present_continuity_reason: p_psr(s.present_continuity_reason),
        present_continuity_actor: cos(&s.present_continuity_actor),
        future_continuity_reason: p_pstc(s.future_continuity_reason),
    }
}
fn p_portstatus(s: &lib_tms::PowerPortStatus) -> patch::PowerPortStatus {
    patch::PowerPortStatus {
        port_number: s.port_number,
        cable_status: p_cable(s.cable_status),
        cable_id: cos(&s.cable_id),
        connection: s.connection.as_ref().map(p_portid),
        switch_status: s.switch_status.as_ref().map(p_switch),
    }
}
fn p_dtc(d: &lib_tms::DiagnosticTroubleCode) -> patch::DiagnosticTroubleCode {
    patch::DiagnosticTroubleCode {
        port_number: d.port_number,
        suspect_parameter: d.suspect_parameter,
        failure_mode: p_fmi(d.failure_mode),
        occurrence_count: d.occurrence_count,
        time_raised: p_clock(&d.time_raised),
        severity: p_sev(d.severity),
        estimate_time: d.estimate_time.as_ref().map(p_clock),
        estimate_severity: d.estimate_severity.map(p_sev),
        hint: cs(&d.hint),
    }
}
fn p_param(p: &lib_tms::ParameterMetadata) -> patch::ParameterMetadata {
    patch::ParameterMetadata {
        name: cs(&p.name),
        units: cs(&p.units),
        nominal_min_value: p.nominal_min_value,
        nominal_max_value: p.nominal_max_value,
        hard_min_value: p.hard_min_value,
        hard_max_value: p.hard_max_value,
        resolution: p.resolution,
        enum_labels: p
            .enum_labels
            .iter()
            .map(|l| l.as_str().to_owned())
            .collect(),
    }
}
fn p_product(p: &lib_tms::ProductInfo) -> patch::ProductInfo {
    patch::ProductInfo {
        nsn: cs(&p.nsn),
        gtin: cs(&p.gtin),
        manufacturer_name: cs(&p.manufacturer_name),
        model_name: cs(&p.model_name),
        model_number: cs(&p.model_number),
        serial_number: cs(&p.serial_number),
        software_version: cs(&p.software_version),
        platform_id: cos(&p.platform_id),
    }
}
fn p_topics(t: &lib_tms::TopicInfo) -> patch::TopicInfo {
    patch::TopicInfo {
        data_model_version: cs(&t.data_model_version),
        published_conditional_topics: t
            .published_conditional_topics
            .iter()
            .map(|s| s.as_str().to_owned())
            .collect(),
        published_optional_topics: t
            .published_optional_topics
            .iter()
            .map(|s| s.as_str().to_owned())
            .collect(),
        supported_request_topics: t
            .supported_request_topics
            .iter()
            .map(|s| s.as_str().to_owned())
            .collect(),
        extension_topics: t
            .extension_topics
            .as_ref()
            .map(|v| v.iter().map(|s| s.as_str().to_owned()).collect()),
    }
}
fn p_ctrl_hw(c: &lib_tms::ControlHardwareInfo) -> patch::ControlHardwareInfo {
    patch::ControlHardwareInfo {
        has_realtime_clock: c.has_realtime_clock,
        min_temperature: c.min_temperature,
        max_temperature: c.max_temperature,
        num_network_ports: c.num_network_ports,
    }
}

// ==================================================================================================
// small fixtures
// ==================================================================================================

fn clock(sec: u32, nsec: u32) -> lib_tms::ClockMonotonic {
    lib_tms::ClockMonotonic {
        seconds: sec,
        nanoseconds: nsec,
    }
}

fn curve(a: f32, b: f32) -> lib_tms::Curve2D {
    lib_tms::Curve2D {
        points: hvec([
            lib_tms::Point2D { x: a, y: b },
            lib_tms::Point2D {
                x: a + 1.0,
                y: b + 1.0,
            },
        ]),
    }
}

// ==================================================================================================
// (1) ActiveMicrogridControllerState — the smallest @optional topic (single Option<String>)
// ==================================================================================================

fn amcs(master: Option<&str>) -> lib_tms::ActiveMicrogridControllerState {
    lib_tms::ActiveMicrogridControllerState {
        device_id: hstr("MC-1"),
        master_id: master.map(|s| hstr(s)),
    }
}
fn p_amcs(v: &lib_tms::ActiveMicrogridControllerState) -> patch::ActiveMicrogridControllerState {
    patch::ActiveMicrogridControllerState {
        device_id: cs(&v.device_id),
        master_id: cos(&v.master_id),
    }
}

#[test]
fn active_microgrid_controller_state_present_and_absent() {
    let present = amcs(Some("MASTER-9"));
    check(
        "ActiveMicrogridControllerState(present)",
        &present,
        p_amcs(&present),
    );
    let absent = amcs(None);
    check(
        "ActiveMicrogridControllerState(absent)",
        &absent,
        p_amcs(&absent),
    );
}

// ==================================================================================================
// (2) EnergyStartStopState — one mid-struct Option<String> (present_level_actor) followed by a
//     REQUIRED enum (future_level_reason): the non-trailing 0x00 shape.
// ==================================================================================================

fn esss(actor: Option<&str>) -> lib_tms::EnergyStartStopState {
    lib_tms::EnergyStartStopState {
        device_id: hstr("ESS-1"),
        present_level: lib_tms::EnergyStartStopLevel::Operational,
        future_level: lib_tms::EnergyStartStopLevel::Ready,
        request_lock: true,
        elapsed_time: 3.0,
        remaining_time: 4.0,
        present_level_reason: lib_tms::PowerSwitchReason::Manual,
        present_level_actor: actor.map(|s| hstr(s)),
        future_level_reason: lib_tms::PowerSwitchTransitionCode::Complete,
    }
}
fn p_esss(v: &lib_tms::EnergyStartStopState) -> patch::EnergyStartStopState {
    patch::EnergyStartStopState {
        device_id: cs(&v.device_id),
        present_level: p_essl(v.present_level),
        future_level: p_essl(v.future_level),
        request_lock: v.request_lock,
        elapsed_time: v.elapsed_time,
        remaining_time: v.remaining_time,
        present_level_reason: p_psr(v.present_level_reason),
        present_level_actor: cos(&v.present_level_actor),
        future_level_reason: p_pstc(v.future_level_reason),
    }
}

#[test]
fn energy_start_stop_state_present_and_absent() {
    let present = esss(Some("OPERATOR-7"));
    check("EnergyStartStopState(present)", &present, p_esss(&present));
    let absent = esss(None);
    check("EnergyStartStopState(absent)", &absent, p_esss(&absent));
}

// ==================================================================================================
// (3) StorageUpdate — three NON-CONTIGUOUS Option<f32> (state_of_charge early, max_charge_rate /
//     max_discharge_rate trailing) around four required Curve2D members.
// ==================================================================================================

fn storage_update(soc: Option<f32>, rates: Option<(f32, f32)>) -> lib_tms::StorageUpdate {
    lib_tms::StorageUpdate {
        device_id: hstr("STOR-1"),
        internal_voltage: 48.0,
        state_of_charge: soc,
        available_energy: 100.0,
        hold_time: 200.0,
        charge_time: curve(1.0, 2.0),
        discharge_time: curve(3.0, 4.0),
        max_charge_time: curve(5.0, 6.0),
        max_discharge_time: curve(7.0, 8.0),
        max_charge_rate: rates.map(|r| r.0),
        max_discharge_rate: rates.map(|r| r.1),
    }
}
fn p_storage_update(v: &lib_tms::StorageUpdate) -> patch::StorageUpdate {
    patch::StorageUpdate {
        device_id: cs(&v.device_id),
        internal_voltage: v.internal_voltage,
        state_of_charge: v.state_of_charge,
        available_energy: v.available_energy,
        hold_time: v.hold_time,
        charge_time: p_curve(&v.charge_time),
        discharge_time: p_curve(&v.discharge_time),
        max_charge_time: p_curve(&v.max_charge_time),
        max_discharge_time: p_curve(&v.max_discharge_time),
        max_charge_rate: v.max_charge_rate,
        max_discharge_rate: v.max_discharge_rate,
    }
}

#[test]
fn storage_update_present_and_absent() {
    let present = storage_update(Some(0.85), Some((10.0, 11.0)));
    check(
        "StorageUpdate(present)",
        &present,
        p_storage_update(&present),
    );
    let absent = storage_update(None, None);
    check("StorageUpdate(absent)", &absent, p_storage_update(&absent));
}

// ==================================================================================================
// (4) PowerPortState → PowerPortStatus → PowerSwitchStatus — the MID-STRUCT optional case. The
//     PowerPortStatus optionals (cable_id, connection, switch_status) plus the PowerSwitchStatus
//     mid-struct optional (present_continuity_actor, followed by the REQUIRED future_continuity_reason)
//     exercise a non-trailing 0x00 inside a sequence element.
// ==================================================================================================

fn switch_status(actor: Option<&str>) -> lib_tms::PowerSwitchStatus {
    lib_tms::PowerSwitchStatus {
        present_continuity: lib_tms::CircuitContinuity::Closed,
        future_continuity: lib_tms::CircuitContinuity::Open,
        request_lock: false,
        present_continuity_reason: lib_tms::PowerSwitchReason::Manual,
        present_continuity_actor: actor.map(|s| hstr(s)),
        future_continuity_reason: lib_tms::PowerSwitchTransitionCode::Complete,
    }
}

fn power_port_state(present: bool) -> lib_tms::PowerPortState {
    let status = if present {
        lib_tms::PowerPortStatus {
            port_number: 1,
            cable_status: lib_tms::CableSenseStatus::Connected,
            cable_id: Some(hstr("CBL-1")),
            connection: Some(lib_tms::PowerPortId {
                device_id: hstr("PEER-2"),
                port_number: 3,
            }),
            // switch_status present, but its mid-struct actor absent -> non-trailing 0x00.
            switch_status: Some(switch_status(None)),
        }
    } else {
        lib_tms::PowerPortStatus {
            port_number: 1,
            cable_status: lib_tms::CableSenseStatus::Connected,
            cable_id: None,
            connection: None,
            switch_status: None,
        }
    };
    lib_tms::PowerPortState {
        device_id: hstr("PP-1"),
        power_ports: hvec([status]),
    }
}
fn p_power_port_state(v: &lib_tms::PowerPortState) -> patch::PowerPortState {
    patch::PowerPortState {
        device_id: cs(&v.device_id),
        power_ports: v.power_ports.iter().map(p_portstatus).collect(),
    }
}

#[test]
fn power_port_state_present_and_absent() {
    let present = power_port_state(true);
    check(
        "PowerPortState(present)",
        &present,
        p_power_port_state(&present),
    );
    let absent = power_port_state(false);
    check(
        "PowerPortState(absent)",
        &absent,
        p_power_port_state(&absent),
    );
}

// ==================================================================================================
// (5) ActiveDiagnosticState → DiagnosticTroubleCode — a LEADING optional (port_number = member 0)
//     plus two trailing Option members (estimate_time, estimate_severity), inside a sequence.
// ==================================================================================================

fn dtc(present: bool) -> lib_tms::DiagnosticTroubleCode {
    lib_tms::DiagnosticTroubleCode {
        port_number: if present { Some(2) } else { None },
        suspect_parameter: 4096,
        failure_mode: lib_tms::FailureModeIndicator::VoltageHigh,
        occurrence_count: 5,
        time_raised: clock(1000, 0),
        severity: lib_tms::DtcSeverity::Warning,
        estimate_time: if present { Some(clock(2000, 0)) } else { None },
        estimate_severity: if present {
            Some(lib_tms::DtcSeverity::Critical)
        } else {
            None
        },
        hint: hstr("check wiring"),
    }
}
fn active_diagnostic_state(present: bool) -> lib_tms::ActiveDiagnosticState {
    lib_tms::ActiveDiagnosticState {
        device_id: hstr("DIAG-1"),
        codes: hvec([dtc(present)]),
        overflow: 0,
    }
}
fn p_active_diagnostic_state(v: &lib_tms::ActiveDiagnosticState) -> patch::ActiveDiagnosticState {
    patch::ActiveDiagnosticState {
        device_id: cs(&v.device_id),
        codes: v.codes.iter().map(p_dtc).collect(),
        overflow: v.overflow,
    }
}

#[test]
fn active_diagnostic_state_present_and_absent() {
    let present = active_diagnostic_state(true);
    check(
        "ActiveDiagnosticState(present)",
        &present,
        p_active_diagnostic_state(&present),
    );
    let absent = active_diagnostic_state(false);
    check(
        "ActiveDiagnosticState(absent)",
        &absent,
        p_active_diagnostic_state(&absent),
    );
}

// ==================================================================================================
// (6) PowerHardwareUpdate — six nested-STRUCT optionals; the present case sets all six Some, and its
//     EngineState carries two NON-ADJACENT Option<f32> (wet_stack .. run_time .. fuel_consumption_rate)
//     and its EnergyStorageState carries a StorageCellState with its own Option<f32>.
// ==================================================================================================

fn power_hardware_update(present: bool) -> lib_tms::PowerHardwareUpdate {
    if present {
        lib_tms::PowerHardwareUpdate {
            device_id: hstr("PHW-1"),
            timestamp: clock(500, 250),
            engine: Some(lib_tms::EngineState {
                operating_state: lib_tms::EngineOperatingState::Running,
                oil_pressure: 40.0,
                coolant_temperature: 80.0,
                speed: 1800.0,
                wet_stack: Some(0.2),
                run_time: 3600.0,
                fuel_consumption_rate: Some(5.5),
            }),
            fuel: Some(lib_tms::FuelState {
                fuel_level: 75.0,
                fuel_pump_running: true,
            }),
            generator: Some(lib_tms::GeneratorState {
                field_current: 10.0,
                stator_temperature: 65.0,
            }),
            energy_storage: Some(lib_tms::EnergyStorageState {
                min_temperature: 20.0,
                average_temperature: 25.0,
                max_temperature: 30.0,
                run_time: 100.0,
                storage_cells: hvec([lib_tms::StorageCellState {
                    cell_number: 1,
                    cycle_counter: 12.0,
                    state_of_charge: Some(0.9),
                    temperature: 26.0,
                    voltage: 3.7,
                }]),
            }),
            power_electronics: Some(lib_tms::PowerElectronicsState { temperature: 55.0 }),
            thermal: Some(lib_tms::ThermalState {
                thermal_load: hvec([1.0, 2.0, 3.0]),
            }),
        }
    } else {
        lib_tms::PowerHardwareUpdate {
            device_id: hstr("PHW-1"),
            timestamp: clock(500, 250),
            engine: None,
            fuel: None,
            generator: None,
            energy_storage: None,
            power_electronics: None,
            thermal: None,
        }
    }
}
fn p_power_hardware_update(v: &lib_tms::PowerHardwareUpdate) -> patch::PowerHardwareUpdate {
    patch::PowerHardwareUpdate {
        device_id: cs(&v.device_id),
        timestamp: p_clock(&v.timestamp),
        engine: v.engine.as_ref().map(p_engine),
        fuel: v.fuel.as_ref().map(p_fuel),
        generator: v.generator.as_ref().map(p_gen),
        energy_storage: v.energy_storage.as_ref().map(p_ess),
        power_electronics: v.power_electronics.as_ref().map(p_pe),
        thermal: v.thermal.as_ref().map(p_thermal),
    }
}

#[test]
fn power_hardware_update_present_and_absent() {
    let present = power_hardware_update(true);
    check(
        "PowerHardwareUpdate(present)",
        &present,
        p_power_hardware_update(&present),
    );
    let absent = power_hardware_update(false);
    check(
        "PowerHardwareUpdate(absent)",
        &absent,
        p_power_hardware_update(&absent),
    );
}

// ==================================================================================================
// (7) DeviceInfo — the deepest closure. Present case: all SIX top-level optionals Some, but their
//     OWN sub-optionals absent (power_hardware = all-None, control_service = both-None, power_device
//     = empty vecs + all-None) — so 0x01 presence at depth 1 wraps a cascade of 0x00 at depth 2+.
//     product.platform_id and topics.extension_topics are Some, exercising nested-present bytes.
//     Absent case: all six top-level optionals None.
// ==================================================================================================

fn product_info(platform: Option<&str>) -> lib_tms::ProductInfo {
    lib_tms::ProductInfo {
        nsn: hstr("1234567890123"),
        gtin: hstr("01234567890123"),
        manufacturer_name: hstr("ACME"),
        model_name: hstr("MDL"),
        model_number: hstr("MN-1"),
        serial_number: hstr("SN-1"),
        software_version: hstr("1.0.0"),
        platform_id: platform.map(|s| hstr(s)),
    }
}
fn topic_info(ext: Option<&[&str]>) -> lib_tms::TopicInfo {
    lib_tms::TopicInfo {
        data_model_version: hstr("2.0.2"),
        published_conditional_topics: hvec([hstr::<64>("PowerHardwareUpdate")]),
        published_optional_topics: hvec([hstr::<64>("DeviceInfo")]),
        supported_request_topics: hvec([hstr::<64>("DeviceConfigRequest")]),
        extension_topics: ext.map(|xs| hvec(xs.iter().copied().map(|s| hstr::<64>(s)))),
    }
}
fn control_hardware_info() -> lib_tms::ControlHardwareInfo {
    lib_tms::ControlHardwareInfo {
        has_realtime_clock: true,
        min_temperature: -20.0,
        max_temperature: 70.0,
        num_network_ports: 2,
    }
}
fn parameter_metadata(name: &str) -> lib_tms::ParameterMetadata {
    lib_tms::ParameterMetadata {
        name: hstr(name),
        units: hstr("V"),
        nominal_min_value: 0.0,
        nominal_max_value: 100.0,
        hard_min_value: -10.0,
        hard_max_value: 110.0,
        resolution: 0.1,
        enum_labels: hvec([hstr::<32>("LOW"), hstr::<32>("HIGH")]),
    }
}

/// PowerHardwareInfo with every optional absent (its data is irrelevant — only its PRESENCE at the
/// DeviceInfo level matters; keeping it all-None keeps the deep closure tractable while still
/// emitting a 0x01 at depth 1 wrapping 7 x 0x00 at depth 2).
fn power_hardware_info_empty() -> lib_tms::PowerHardwareInfo {
    lib_tms::PowerHardwareInfo {
        engine: None,
        fuel: None,
        generator: None,
        energy_storage: None,
        power_electronics: None,
        thermal: None,
        synchronous_machine_model: None,
    }
}
fn control_service_info_empty() -> lib_tms::ControlServiceInfo {
    lib_tms::ControlServiceInfo { mc: None, md: None }
}
fn power_device_info_empty() -> lib_tms::PowerDeviceInfo {
    lib_tms::PowerDeviceInfo {
        power_ports: heapless::Vec::new(),
        grounds: heapless::Vec::new(),
        conversion: None,
        distribution: None,
        source: None,
        storage: None,
        load: None,
    }
}

fn device_info(present: bool) -> lib_tms::DeviceInfo {
    lib_tms::DeviceInfo {
        device_id: hstr("DEV-1"),
        role: lib_tms::DeviceRole::Source,
        product: product_info(if present { Some("PLT-1") } else { None }),
        topics: topic_info(if present { Some(&["ExtraTopic"]) } else { None }),
        control_hardware: if present {
            Some(control_hardware_info())
        } else {
            None
        },
        power_hardware: if present {
            Some(power_hardware_info_empty())
        } else {
            None
        },
        control_parameters: if present {
            Some(hvec([parameter_metadata("ctrl_a")]))
        } else {
            None
        },
        metric_parameters: if present {
            Some(hvec([parameter_metadata("metric_a")]))
        } else {
            None
        },
        control_service: if present {
            Some(control_service_info_empty())
        } else {
            None
        },
        power_device: if present {
            Some(power_device_info_empty())
        } else {
            None
        },
    }
}

fn p_device_info(v: &lib_tms::DeviceInfo) -> patch::DeviceInfo {
    patch::DeviceInfo {
        device_id: cs(&v.device_id),
        role: p_role(v.role),
        product: p_product(&v.product),
        topics: p_topics(&v.topics),
        control_hardware: v.control_hardware.as_ref().map(p_ctrl_hw),
        // Presence (Some/None) is mapped from the lib value; the all-absent body matches the lib
        // side's all-None construction, so byte-identity + round-trip hold.
        power_hardware: v.power_hardware.as_ref().map(|_| patch::PowerHardwareInfo {
            engine: None,
            fuel: None,
            generator: None,
            energy_storage: None,
            power_electronics: None,
            thermal: None,
            synchronous_machine_model: None,
        }),
        control_parameters: v
            .control_parameters
            .as_ref()
            .map(|ps| ps.iter().map(p_param).collect()),
        metric_parameters: v
            .metric_parameters
            .as_ref()
            .map(|ps| ps.iter().map(p_param).collect()),
        control_service: v
            .control_service
            .as_ref()
            .map(|_| patch::ControlServiceInfo { mc: None, md: None }),
        power_device: v.power_device.as_ref().map(|_| patch::PowerDeviceInfo {
            power_ports: Vec::new(),
            grounds: Vec::new(),
            conversion: None,
            distribution: None,
            source: None,
            storage: None,
            load: None,
        }),
    }
}

#[test]
fn device_info_all_present_and_all_absent() {
    let present = device_info(true);
    check("DeviceInfo(all-present)", &present, p_device_info(&present));
    let absent = device_info(false);
    check("DeviceInfo(all-absent)", &absent, p_device_info(&absent));
}

// ==================================================================================================
// (optional) live CycloneDDS #323 golden-decode — env-gated; skips unless a hex dump is supplied.
// No binary golden vector for DeviceInfo is checked in under deploy/dds-conformance/ (only Heartbeat
// / DcMeasurementUpdate text artifacts), so this proves the vendored deserializer consumes a REAL
// CycloneDDS-emitted DeviceInfo payload when one is provided.
// ==================================================================================================

#[test]
fn device_info_cyclonedds_golden_decode_if_present() {
    let Ok(path) = std::env::var("SENTIENT_DDS_GOLDEN_DEVICEINFO") else {
        eprintln!(
            "SKIP device_info_cyclonedds_golden_decode_if_present: set \
             SENTIENT_DDS_GOLDEN_DEVICEINFO=<hex-dump path> (a CycloneDDS-emitted DeviceInfo XCDR2 \
             SerializedPayload, whitespace-separated hex bytes) to run — none checked in under \
             deploy/dds-conformance/."
        );
        return;
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read golden {path}: {e}"));
    let bytes: Vec<u8> = text
        .split_whitespace()
        .map(|tok| u8::from_str_radix(tok.trim_start_matches("0x"), 16).expect("hex byte"))
        .collect();
    let dynamic = CdrDeserializer::deserialize(patch::DeviceInfo::get_type(), &bytes)
        .expect("decode CycloneDDS golden DeviceInfo");
    let sample = patch::DeviceInfo::create_sample(dynamic);
    assert!(
        !sample.device_id.is_empty(),
        "golden DeviceInfo decoded but device_id (@key) is empty"
    );
    println!(
        "PASS: decoded CycloneDDS golden DeviceInfo device_id={:?}",
        sample.device_id
    );
}

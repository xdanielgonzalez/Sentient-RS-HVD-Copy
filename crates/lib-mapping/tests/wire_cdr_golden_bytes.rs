//! FROZEN GOLDEN XCDR2 BYTES — the full RTPS SerializedPayload our generated CDR chains emit for a
//! representative fixture per topic, committed under `tests/fixtures/xcdr2/*.hex`.
//!
//! PROVENANCE: these vectors were authored FROM our chain (`lib_mapping::render_cdr`) once
//! `wire_cdr_cross_oracle` proved the chain byte-exact against the dust_dds 0.15.0 reference for
//! every OPTIONAL-FREE topic, and against hand-derived spec vectors for the `@optional` mechanic. So
//! the OPTIONAL-FREE topics here are reference-validated; the OPTIONAL-BEARING topics
//! (PowerHardwareUpdate, StorageUpdate, ActiveDiagnosticState-with-DTCs, the inverter topics with
//! absent `phase_offset`, DeviceInfo, EnergyStartStopState) encode our spec-literal presence-flag
//! form (`lib_tms::cdr::emit_optional`).
//!
//! **WP5 CycloneDDS arbitration (story #323) — RESOLVED, NO FLIP, authority-confirmed (no byte
//! change needed):** CycloneDDS 11.0.1 (eclipse-cyclonedds Python binding) independently encodes the
//! IDENTICAL presence-flag form for `@optional` members — verified against the minimal
//! `ActiveMicrogridControllerState.master_id` case (present AND absent, byte-exact incl. decode
//! acceptance of our exact committed vectors) AND against two REAL golden fixtures here:
//! `energy_start_stop_state` (an absent optional) and `power_hardware_inverter` (a PRESENT optional
//! with a real value, `power_electronics.temperature`) — both byte-exact against a CycloneDDS dump,
//! confirming this is not just the minimal case. `deploy/dds-probe/dump_reference.py`'s
//! `arbitration`/`fixtures` modes reproduce all of this; see `wire_cdr_cross_oracle::
//! OPTIONAL_ENCODING_POLICY` for the policy statement + `lib_tms::cdr` module doc item 3 for the
//! authoritative record.
//!
//! **XTypes rule (12) NEXTINT — FIXED (WP6.5).** CycloneDDS's encoder prefixes every
//! sequence-of-non-primitive-element field (struct or string elements — e.g.
//! `DcMeasurementUpdate.external_measurement`, `ActiveDiagnosticState.codes`,
//! `DeviceInfo.topics.published_*_topics`) with a 4-byte NEXTINT/DHEADER byte-length delimiter
//! (DDS-XTypes 1.3 §7.4.3.5.3 rule (12)). WP7a proved CycloneDDS 11.0.1's DECODER *rejects*
//! sequences without it (live `DcMeasurementUpdate` deserialization failure), so it is mandatory for
//! interop; dust_dds 0.15.0 omitted it (the live dispute eclipse-cyclonedds/cyclonedds-cxx#218 / OMG
//! erratum DDSXTY14-56). WP6.5 RE-ADOPTED the delimiter in `lib_tms::cdr::emit_appendable_seq`/
//! `emit_string_seq` AND vendored+patched dust_dds to emit it too (`vendor/dust_dds/PATCHES.md`), so
//! `chains == dust_dds(patched) == what CycloneDDS accepts` holds again. The 8 fixtures with a
//! non-primitive sequence were regenerated (each grew by 4 bytes per such sequence): `device_info`,
//! `dc_measurement`, `power_hardware`, `active_diagnostic`, `active_diagnostic_inverter`,
//! `storage_update`, `ac_measurement_inverter`, `dc_measurement_inverter`. This is orthogonal to
//! `emit_optional`/`emit_optional_seq` (the WP5 optional-flip point) — the optional-only fixtures
//! (`energy_start_stop_state`, `power_hardware_inverter`, `heartbeat`, `reply`) are byte-unchanged.
//!
//! Regenerate after an intentional wire change: `cargo test -p lib-mapping --test
//! wire_cdr_golden_bytes -- --ignored regenerate_golden_fixtures` then re-run this suite.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::all)]

use lib_mapping::render::TmsTopic;
use lib_mapping::render_cdr::{
    render_ac_measurement_inverter_cdr, render_active_diagnostic_inverter_cdr, render_cdr_value,
    render_dc_measurement_inverter_cdr, render_fault_cdr, render_power_hardware_inverter_cdr,
    render_storage_update_cdr, render_tms_cdr,
};
use lib_mapping::schema::{
    ConfigId, DeviceInfo, DeviceRole, EnergyStartStopLevel, EnergyStartStopState, Heartbeat,
    PowerSwitchReason, PowerSwitchTransitionCode, ProductInfo, Reply, ReplyCode, ReplyStatus,
    TopicInfo,
};
use lib_parse::canonical::{
    CanonicalFault, CanonicalInverterFault, CanonicalInverterReading, CanonicalReading,
    InverterMode,
};
use lib_tms::{ClockMonotonic, Identity};

const FIXTURE_DIR: &str = "tests/fixtures/xcdr2";

// ---- fixture builders (shared with wire_cdr_cross_oracle / wire_serde_cross_oracle) ----

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

fn inverter_reading() -> CanonicalInverterReading {
    CanonicalInverterReading {
        grid_voltage_dv: 2300,
        grid_frequency_dhz: 500,
        ac_output_voltage_dv: 2300,
        ac_output_frequency_dhz: 500,
        ac_output_apparent_power_va: 5000,
        ac_output_active_power_w: 4500,
        bus_voltage_v: 400,
        battery_voltage_cv: 5100,
        battery_charging_current_a: 400,
        battery_discharge_current_a: 200,
        inverter_temperature_c: 350,
        pv_current_da: 120,
        pv_voltage_dv: 4800,
        pv_charging_power_w: 5760,
        status_bits: 0,
        mode: InverterMode::Line,
        ratings: None,
    }
}

fn inverter_fault() -> CanonicalInverterFault {
    CanonicalInverterFault {
        line_fail: true,
        overtemperature_fault: true,
        fan_locked_fault: false,
        battery_low_fault: false,
        overload_fault: false,
        output_short_fault: false,
        raw_bits: 0,
    }
}

fn heartbeat() -> Heartbeat {
    Heartbeat {
        device_id: dev("METEOR-001"),
        sequence_number: 42,
    }
}

fn reply() -> Reply {
    Reply {
        requesting_device_id: dev("REQ-1"),
        target_device_id: dev("TGT-1"),
        config: ConfigId::Active,
        port_number: 7,
        request_sequence_id: 0x0102_0304_0506_0708,
        status: ReplyStatus {
            code: ReplyCode::NotImplemented,
            reason: heapless::String::try_from("nope").unwrap(),
        },
    }
}

fn energy_start_stop_state() -> EnergyStartStopState {
    EnergyStartStopState {
        device_id: dev("METEOR-001"),
        present_level: EnergyStartStopLevel::Operational,
        future_level: EnergyStartStopLevel::Operational,
        request_lock: false,
        elapsed_time: 0.0,
        remaining_time: f32::NAN,
        present_level_reason: PowerSwitchReason::Startup,
        present_level_actor: None,
        future_level_reason: PowerSwitchTransitionCode::None,
    }
}

fn device_info() -> DeviceInfo {
    DeviceInfo {
        device_id: dev("METEOR-001"),
        role: DeviceRole::Storage,
        product: ProductInfo {
            nsn: heapless::String::new(),
            gtin: heapless::String::new(),
            manufacturer_name: heapless::String::try_from("HVD").unwrap(),
            model_name: heapless::String::try_from("METEOR").unwrap(),
            model_number: heapless::String::new(),
            serial_number: heapless::String::try_from("SN-0001").unwrap(),
            software_version: heapless::String::try_from("0.3.0").unwrap(),
            platform_id: None,
        },
        topics: TopicInfo {
            data_model_version: heapless::String::try_from("2.0.2").unwrap(),
            published_conditional_topics: heapless::Vec::new(),
            published_optional_topics: heapless::Vec::new(),
            supported_request_topics: heapless::Vec::new(),
            extension_topics: None,
        },
        control_hardware: None,
        power_hardware: None,
        power_device: None,
        control_parameters: None,
        metric_parameters: None,
        control_service: None,
    }
}

/// Render each fixture topic to its full XCDR2 SerializedPayload. `(fixture_name, bytes)`.
fn all_fixtures() -> Vec<(&'static str, Vec<u8>)> {
    fn go(f: impl FnOnce(&mut [u8]) -> Result<usize, lib_mapping::codec::CodecError>) -> Vec<u8> {
        let mut buf = [0u8; 16 * 1024];
        let n = f(&mut buf).expect("render");
        buf[..n].to_vec()
    }
    let r = reading(14, 6, -164, 5452, 775, 2);
    let d = dev("METEOR-001");
    let t = now(12345, 678_000_000);
    let inv = inverter_reading();
    let invf = inverter_fault();
    let ti = now(54321, 123_456_789);

    vec![
        ("heartbeat", go(|b| render_cdr_value(&heartbeat(), b))),
        ("reply", go(|b| render_cdr_value(&reply(), b))),
        ("device_info", go(|b| render_cdr_value(&device_info(), b))),
        (
            "energy_start_stop_state",
            go(|b| render_cdr_value(&energy_start_stop_state(), b)),
        ),
        (
            "dc_measurement",
            go(|b| render_tms_cdr(TmsTopic::DcMeasurementUpdate, d.clone(), &r, t, b)),
        ),
        (
            "power_hardware",
            go(|b| render_tms_cdr(TmsTopic::PowerHardwareUpdate, d.clone(), &r, t, b)),
        ),
        (
            "active_diagnostic",
            go(|b| render_fault_cdr(&fault(0xFFFF), d.clone(), t, b)),
        ),
        (
            "storage_update",
            go(|b| render_storage_update_cdr(d.clone(), &r, b)),
        ),
        (
            "ac_measurement_inverter",
            go(|b| render_ac_measurement_inverter_cdr(dev("INV-001"), &inv, ti, b)),
        ),
        (
            "dc_measurement_inverter",
            go(|b| render_dc_measurement_inverter_cdr(dev("INV-001"), &inv, ti, b)),
        ),
        (
            "power_hardware_inverter",
            go(|b| render_power_hardware_inverter_cdr(dev("INV-001"), &inv, ti, b)),
        ),
        (
            "active_diagnostic_inverter",
            go(|b| {
                render_active_diagnostic_inverter_cdr(
                    &invf,
                    InverterMode::Line,
                    dev("INV-001"),
                    ti,
                    b,
                )
            }),
        ),
    ]
}

fn to_hex(b: &[u8]) -> String {
    b.iter()
        .map(|x| format!("{x:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn from_hex(s: &str) -> Vec<u8> {
    s.split_whitespace()
        .map(|t| u8::from_str_radix(t, 16).expect("hex byte"))
        .collect()
}

fn fixture_path(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(FIXTURE_DIR)
        .join(format!("{name}.hex"))
}

/// FROZEN: each committed `fixtures/xcdr2/<name>.hex` must byte-equal the current chain output.
#[test]
fn golden_xcdr2_bytes_match() {
    let mut missing = Vec::new();
    let mut mismatched = Vec::new();
    for (name, bytes) in all_fixtures() {
        let path = fixture_path(name);
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                let expected = from_hex(text.trim());
                if expected != bytes {
                    mismatched.push(format!(
                        "  {name}:\n    golden ({}): {}\n    actual ({}): {}",
                        expected.len(),
                        to_hex(&expected),
                        bytes.len(),
                        to_hex(&bytes)
                    ));
                }
            }
            Err(_) => missing.push(name),
        }
    }
    assert!(
        missing.is_empty(),
        "missing golden fixtures {missing:?} — run `cargo test -p lib-mapping --test \
         wire_cdr_golden_bytes -- --ignored regenerate_golden_fixtures`"
    );
    assert!(
        mismatched.is_empty(),
        "XCDR2 golden byte mismatch (intentional wire change? regenerate + re-review):\n{}",
        mismatched.join("\n")
    );
}

/// Every fixture is a well-formed RTPS SerializedPayload: >= 4 bytes, D_CDR2_LE encapsulation id,
/// and total length a multiple of 4 (trailing encapsulation padding).
#[test]
fn golden_xcdr2_bytes_well_formed() {
    for (name, bytes) in all_fixtures() {
        assert!(bytes.len() >= 8, "{name}: too short");
        assert_eq!(&bytes[0..2], &[0x00, 0x09], "{name}: not D_CDR2_LE");
        assert_eq!(bytes.len() % 4, 0, "{name}: not 4-aligned");
    }
}

/// AUTHORING step (ignored): (re)write the committed `.hex` fixtures from the current chain output.
/// Run deliberately after an intentional wire change; the committed fixtures are the frozen oracle.
#[test]
#[ignore = "authoring step — writes committed fixtures"]
fn regenerate_golden_fixtures() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE_DIR);
    std::fs::create_dir_all(&dir).expect("create fixtures dir");
    for (name, bytes) in all_fixtures() {
        let path = fixture_path(name);
        std::fs::write(&path, format!("{}\n", to_hex(&bytes))).expect("write fixture");
        eprintln!("wrote {} ({} bytes)", path.display(), bytes.len());
    }
}

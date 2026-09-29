//! Steps for TMS model-breadth feature area — Heartbeat, DeviceInfo family, StorageUpdate, and
//! the Request/Reply control path (see `docs/gap-analysis-meteor-parity.md`'s "TMS data models"
//! section).
//!
//! Pure in-process, the same convention `steps/tms.rs` establishes: constructed real `lib_tms`
//! types + `serde_json` only, no broker/network. The actual Request/Reply round-trip-under-budget
//! behavior against a real embedded broker is `test-e2e/tests/request_reply_e2e.rs`'s job (see
//! that file's doc comment for why `bin-rpi::tasks::reply_listener` itself can't be imported
//! here — `bin-rpi` ships no library target).
//!
//! Test code only; freely uses unwrap/expect/panic (the workspace lint gate denies these in
//! production logic, not tests).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cucumber::{given, then, when};

use heapless::{String as HString, Vec as HVec};
use lib_mapping::schema::{
    topics, ActiveMicrogridControllerState, CableSenseStatus, CircuitWiring, ConfigId,
    DcLoadSharingState, DeviceInfo, DeviceRole, EnergyStartStopLevel, EnergyStartStopState,
    Heartbeat, Identity, LoadSharingInfo, PowerConnectorInfo, PowerConnectorPolarity,
    PowerConnectorType, PowerDeviceInfo, PowerPortDirectionality, PowerPortInfo, PowerPortState,
    PowerPortStatus, PowerSwitchReason, PowerSwitchTransitionCode, ProductInfo, Reply, ReplyCode,
    ReplyStatus, StorageInfo, String32, TopicInfo, TopicList, INVALID_PORT_NUMBER,
    TMS_DATA_MODEL_VERSION,
};
use lib_mapping::sink::MAX_PAYLOAD;
use lib_mapping::transform::ToStorageUpdate;
use lib_parse::canonical::{CanonicalReading, MAX_CELLS, MAX_TEMPS};

use crate::{dev, now, SpecWorld};

/// Scenario state for the tms_breadth feature area (Heartbeat, DeviceInfo family, StorageUpdate,
/// Request/Reply — #240 #244 #246).
#[derive(Debug, Default)]
pub struct TmsBreadthState {
    /// The JSON most recently produced by a `given_*`/`when_*` step here.
    json: String,
    /// `StorageUpdate` JSON-encode result (into a `MAX_PAYLOAD`-sized buffer): `Ok(bytes_written)`
    /// or the `CodecError` variant name.
    storage_codec_result: Option<Result<usize, String>>,
    /// Heartbeat sequence number under test, mutated by the wrapping-increment step.
    sequence_number: u32,
    /// The requesting device id + request sequence a `given a request ...` step recorded, echoed
    /// by the well-formed-request `Reply`.
    req_requesting_device: String,
    req_sequence: u64,
}

fn hstr32(s: &str) -> String32 {
    let mut h = String32::new();
    h.push_str(s).expect("fits String32");
    h
}

fn hid(s: &str) -> Identity {
    let mut h = Identity::new();
    h.push_str(s).expect("fits Identity");
    h
}

fn htopic(s: &str) -> HString<64> {
    let mut h = HString::new();
    h.push_str(s).expect("topic fits String<64>");
    h
}

fn json_value(world: &SpecWorld) -> serde_json::Value {
    serde_json::from_str(&world.tms_breadth.json).expect("TmsBreadthState::json is valid JSON")
}

/// Resolve a dotted field path (e.g. `"status.code"`, or `"power_ports.0.cable_status"` with a
/// numeric array-index segment) against a JSON value. `serde_json::Value`'s `Index` impl returns
/// `Value::Null` for a missing key rather than panicking, so an unknown segment surfaces as a
/// JSON-null mismatch in the caller's assertion, not a panic here.
fn field_path<'a>(v: &'a serde_json::Value, path: &str) -> &'a serde_json::Value {
    path.split('.')
        .fold(v, |acc, seg| match seg.parse::<usize>() {
            Ok(i) if acc.is_array() => &acc[i],
            _ => &acc[seg],
        })
}

/// A `DeviceInfo` built the same way `bin_rpi::tasks::heartbeat::build_device_info` builds it —
/// STOR role, 8-field `ProductInfo`, an honest `TopicInfo`, and a `power_device` carrying the
/// STOR role's required `storage` member (IDL 5234-5275) plus the one battery `PowerPortInfo`
/// (amperage/voltage bounds NaN, per the standard's "@optional float defaults to NaN"
/// convention, A.9.6.5). Duplicated here as a fixture since `bin-rpi` ships no library target
/// this suite can import (see this file's top doc comment).
fn full_device_info() -> DeviceInfo {
    let product = ProductInfo {
        nsn: HString::new(),
        gtin: HString::new(),
        manufacturer_name: hstr32("HVD"),
        model_name: hstr32("METEOR-STOR"),
        model_number: String32::new(),
        serial_number: hstr32("SN-0001"),
        software_version: hstr32("0.2.3"),
        platform_id: None,
    };

    let mut published_conditional_topics = TopicList::new();
    for t in [
        topics::DC_MEASUREMENT_UPDATE,
        // Conditional-Required on HAS DC PORTS (#263).
        topics::DC_LOAD_SHARING_STATE,
    ] {
        published_conditional_topics.push(htopic(t)).unwrap();
    }
    let mut published_optional_topics = TopicList::new();
    for t in [
        topics::POWER_HARDWARE_UPDATE,
        topics::ACTIVE_DIAGNOSTIC_STATE,
        topics::STORAGE_UPDATE,
        // Required STOR state topics wired end to end in wave 5 (#263).
        topics::ENERGY_START_STOP_STATE,
        topics::POWER_PORT_STATE,
        topics::ACTIVE_MICROGRID_CONTROLLER_STATE,
    ] {
        published_optional_topics.push(htopic(t)).unwrap();
    }

    let topics_info = TopicInfo {
        data_model_version: hstr32(TMS_DATA_MODEL_VERSION),
        published_conditional_topics,
        published_optional_topics,
        supported_request_topics: TopicList::new(),
        extension_topics: None,
    };

    let mut power_ports = HVec::new();
    power_ports
        .push(PowerPortInfo {
            port_number: 0,
            wiring: CircuitWiring::Dc,
            directionality: PowerPortDirectionality::InOut,
            has_switch: false,
            has_external_meter: false,
            has_internal_meter: true,
            has_summary_measurement_update: false,
            min_amperage: f32::NAN,
            max_amperage: f32::NAN,
            short_circuit_amperage: f32::NAN,
            min_voltage: f32::NAN,
            max_voltage: f32::NAN,
            min_frequency: None,
            max_frequency: None,
            connector_info: PowerConnectorInfo {
                features: HVec::new(),
                physical_type: PowerConnectorType::Unknown,
                polarity: PowerConnectorPolarity::Unknown,
            },
            switch_info: None,
        })
        .unwrap();

    let storage = StorageInfo {
        features: HVec::new(),
        max_charge_energy: f32::NAN,
        load_sharing: LoadSharingInfo {
            port_number: 0,
            supports_droop: false,
            supports_multi_segment_droop: false,
            supports_constant_power: false,
            min_real_power: f32::NAN,
            max_real_power: f32::NAN,
            max_overload_real_power: None,
            min_reactive_power: f32::NAN,
            max_reactive_power: f32::NAN,
            max_apparent_power: f32::NAN,
            power_factor: f32::NAN,
            load_response: None,
        },
        supported_energy_start_stop_levels: HVec::new(),
    };

    let power_device = PowerDeviceInfo {
        power_ports,
        grounds: HVec::new(),
        conversion: None,
        distribution: None,
        source: None,
        storage: Some(storage),
        load: None,
    };

    DeviceInfo {
        device_id: dev(),
        role: DeviceRole::Storage,
        product,
        topics: topics_info,
        control_hardware: None,
        power_hardware: None,
        power_device: Some(power_device),
        control_parameters: None,
        metric_parameters: None,
        control_service: None,
    }
}

/// A `CanonicalReading` shaped like the currently-deployed Seplos 48150-1101-10E hardware capture
/// (14 cells / 6 temperature sensors — `docs/seplos-sim.md`'s `LIVE_FRAME`/`sample_14_cell`).
fn deployed_hardware_reading() -> CanonicalReading {
    let mut cells: HVec<u16, MAX_CELLS> = HVec::new();
    for i in 0..14u16 {
        cells.push(3800 + i).expect("14 cells fit MAX_CELLS=16");
    }
    let mut temps: HVec<u16, MAX_TEMPS> = HVec::new();
    for i in 0..6u16 {
        temps.push(2981 + i).expect("6 temps fit MAX_TEMPS=8");
    }
    CanonicalReading {
        device_address: 0,
        cell_voltages_mv: cells,
        temperatures_raw: temps,
        current_ca: -144,
        total_voltage_cv: 5451,
        remaining_capacity_cah: 13953,
        typical_capacity_cah: 18000,
        rated_capacity_cah: 18000,
        soc_dpct: 775,
        soh_dpct: 1000,
        cycles: 2,
        port_voltage_cv: 5453,
    }
}

// ---- Given/When: construct real TMS-breadth types and stash their JSON shape -------------------

#[given("a canonical reading shaped like the deployed 14-cell Seplos hardware")]
fn given_deployed_hardware_reading(world: &mut SpecWorld) {
    world.reading = Some(deployed_hardware_reading());
}

#[given(expr = "a Heartbeat with sequence number {int}")]
fn given_heartbeat(world: &mut SpecWorld, seq: i64) {
    world.tms_breadth.sequence_number = seq as u32;
    let hb = Heartbeat {
        device_id: dev(),
        sequence_number: world.tms_breadth.sequence_number,
    };
    world.tms_breadth.json = serde_json::to_string(&hb).expect("serialize Heartbeat");
}

#[when("the heartbeat sequence number is incremented with wrapping")]
fn when_wrap_sequence(world: &mut SpecWorld) {
    world.tms_breadth.sequence_number = world.tms_breadth.sequence_number.wrapping_add(1);
    let hb = Heartbeat {
        device_id: dev(),
        sequence_number: world.tms_breadth.sequence_number,
    };
    world.tms_breadth.json = serde_json::to_string(&hb).expect("serialize Heartbeat");
}

#[given("a DeviceInfo built from the STOR device config")]
fn given_device_info(world: &mut SpecWorld) {
    world.tms_breadth.json =
        serde_json::to_string(&full_device_info()).expect("serialize DeviceInfo");
}

#[when("the reading is mapped to a StorageUpdate")]
fn when_map_storage_update(world: &mut SpecWorld) {
    let reading = world.reading.clone().expect("reading set");
    let su = reading.to_storage_update(dev(), now());
    world.tms_breadth.json = serde_json::to_string(&su).expect("serialize StorageUpdate");
    let mut buf = vec![0u8; MAX_PAYLOAD];
    world.tms_breadth.storage_codec_result =
        Some(lib_mapping::codec::to_json_slice(&su, &mut buf).map_err(|e| e.to_string()));
}

#[given(expr = "a request from {string} with sequence {int}")]
fn given_request(world: &mut SpecWorld, requester: String, seq: i64) {
    world.tms_breadth.req_requesting_device = requester;
    world.tms_breadth.req_sequence = seq as u64;
}

// ---- Required STOR state publishers (#263) — fixtures mirror `bin_rpi::tasks::state_publisher`,
// duplicated here since `bin-rpi` ships no library target this suite can import ------------------

#[given("an EnergyStartStopState for a healthy STOR device")]
fn given_energy_start_stop_state(world: &mut SpecWorld) {
    let state = EnergyStartStopState {
        device_id: dev(),
        present_level: EnergyStartStopLevel::Operational,
        future_level: EnergyStartStopLevel::Operational,
        request_lock: false,
        elapsed_time: 0.0,
        remaining_time: f32::NAN,
        present_level_reason: PowerSwitchReason::Startup,
        present_level_actor: None,
        future_level_reason: PowerSwitchTransitionCode::None,
    };
    world.tms_breadth.json = serde_json::to_string(&state).expect("serialize EnergyStartStopState");
}

#[given("a PowerPortState for the single declared port")]
fn given_power_port_state(world: &mut SpecWorld) {
    let mut power_ports = HVec::new();
    power_ports
        .push(PowerPortStatus {
            port_number: 0,
            cable_status: CableSenseStatus::Unknown,
            cable_id: None,
            connection: None,
            switch_status: None,
        })
        .unwrap();
    let state = PowerPortState {
        device_id: dev(),
        power_ports,
    };
    world.tms_breadth.json = serde_json::to_string(&state).expect("serialize PowerPortState");
}

#[given("a DcLoadSharingState for a non-load-sharing battery")]
fn given_dc_load_sharing_state(world: &mut SpecWorld) {
    let state = DcLoadSharingState {
        device_id: dev(),
        config: ConfigId::Unknown,
        parameters: HVec::new(),
    };
    world.tms_breadth.json = serde_json::to_string(&state).expect("serialize DcLoadSharingState");
}

#[given(expr = "an ActiveMicrogridControllerState with master {string}")]
fn given_amc_state_with_master(world: &mut SpecWorld, master: String) {
    let state = ActiveMicrogridControllerState {
        device_id: dev(),
        master_id: Some(hid(&master)),
    };
    world.tms_breadth.json =
        serde_json::to_string(&state).expect("serialize ActiveMicrogridControllerState");
}

#[given("an ActiveMicrogridControllerState with no master selected")]
fn given_amc_state_no_master(world: &mut SpecWorld) {
    let state = ActiveMicrogridControllerState {
        device_id: dev(),
        master_id: None,
    };
    world.tms_breadth.json =
        serde_json::to_string(&state).expect("serialize ActiveMicrogridControllerState");
}

#[when("a Reply is built answering the well-formed request with NotImplemented")]
fn when_build_reply_not_implemented(world: &mut SpecWorld) {
    // Mirrors `bin_rpi::tasks::reply_listener::build_reply`'s well-formed branch: echo the
    // requesting id + sequence, config Unknown / port INVALID for a non-config request, and
    // `ReplyCode::NotImplemented` (this adapter exposes no command surface). See that fn's doc.
    let reply = Reply {
        requesting_device_id: hid(&world.tms_breadth.req_requesting_device),
        target_device_id: dev(),
        config: ConfigId::Unknown,
        port_number: INVALID_PORT_NUMBER,
        request_sequence_id: world.tms_breadth.req_sequence,
        status: ReplyStatus {
            code: ReplyCode::NotImplemented,
            reason: hstr32("not implemented"),
        },
    };
    world.tms_breadth.json = serde_json::to_string(&reply).expect("serialize Reply");
}

#[when("a Reply is built for an undecodable request")]
fn when_build_reply_bad_request(world: &mut SpecWorld) {
    // Mirrors `build_reply`'s decode-failure branch: nothing parsed to echo, so an empty requesting
    // id, sequence 0, and `ReplyCode::BadRequest` with a short reason.
    let reply = Reply {
        requesting_device_id: Identity::new(),
        target_device_id: dev(),
        config: ConfigId::Unknown,
        port_number: INVALID_PORT_NUMBER,
        request_sequence_id: 0,
        status: ReplyStatus {
            code: ReplyCode::BadRequest,
            reason: hstr32("malformed request payload"),
        },
    };
    world.tms_breadth.json = serde_json::to_string(&reply).expect("serialize Reply");
}

// ---- Then: JSON field-value assertions ----------------------------------------------------------

#[then(expr = "the TMS breadth JSON field {word} equals {string}")]
fn then_field_equals_string(world: &mut SpecWorld, field: String, expected: String) {
    let v = json_value(world);
    let got = field_path(&v, &field);
    assert_eq!(
        got.as_str(),
        Some(expected.as_str()),
        "field {field} mismatch in {v}"
    );
}

#[then(expr = "the TMS breadth JSON field {word} equals {int}")]
fn then_field_equals_int(world: &mut SpecWorld, field: String, expected: i64) {
    let v = json_value(world);
    let got = field_path(&v, &field);
    assert_eq!(
        got.as_i64(),
        Some(expected),
        "field {field} mismatch in {v}"
    );
}

#[then(expr = "the TMS breadth JSON array {word} is empty")]
fn then_field_array_empty(world: &mut SpecWorld, field: String) {
    let v = json_value(world);
    let got = field_path(&v, &field);
    let arr = got.as_array().expect("field is a JSON array");
    assert!(arr.is_empty(), "expected {field} empty, got {arr:?}");
}

#[then(expr = "the TMS breadth JSON field {word} is null")]
fn then_field_is_null(world: &mut SpecWorld, field: String) {
    let v = json_value(world);
    let got = field_path(&v, &field);
    assert!(got.is_null(), "expected {field} null, got {got} in {v}");
}

#[then(expr = "the TMS breadth JSON array {word} has length {int}")]
fn then_field_array_len(world: &mut SpecWorld, field: String, len: i64) {
    let v = json_value(world);
    let got = field_path(&v, &field);
    let arr = got.as_array().expect("field is a JSON array");
    assert_eq!(arr.len(), len as usize, "array {field} length in {v}");
}

#[then(expr = "the TMS breadth JSON field {word} equals boolean {word}")]
fn then_field_equals_bool(world: &mut SpecWorld, field: String, expected: String) {
    let v = json_value(world);
    let got = field_path(&v, &field);
    let want: bool = expected.parse().expect("boolean literal");
    assert_eq!(got.as_bool(), Some(want), "field {field} mismatch in {v}");
}

#[then(expr = "the StorageUpdate JSON-encodes within the {int}-byte MAX_PAYLOAD budget")]
fn then_storage_update_within_budget(world: &mut SpecWorld, cap: i64) {
    assert_eq!(
        cap as usize, MAX_PAYLOAD,
        "cap literal must match lib_mapping::sink::MAX_PAYLOAD"
    );
    match world
        .tms_breadth
        .storage_codec_result
        .as_ref()
        .expect("StorageUpdate codec ran")
    {
        Ok(n) => assert!(
            *n <= MAX_PAYLOAD,
            "encoded {n} bytes exceeds MAX_PAYLOAD={MAX_PAYLOAD}"
        ),
        Err(e) => {
            panic!("a StorageUpdate must fit MAX_PAYLOAD={MAX_PAYLOAD}, got encode error {e}")
        }
    }
}

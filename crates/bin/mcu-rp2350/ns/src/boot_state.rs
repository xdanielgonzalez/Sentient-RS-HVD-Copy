//! TMS device-identity + Required-STOR-state emitter — closes the MCU↔Pi topic-parity gap.
//!
//! The Seplos poll loop (`bin_mcu_platform::McuPlatform::emit_reading`/`emit_fault`) publishes only
//! the telemetry topics that depend on a fresh reading (`DcMeasurementUpdate`,
//! `PowerHardwareUpdate`, `StorageUpdate`, `ActiveDiagnosticState`). The remaining TMS topics the Pi
//! adapter emits are device-identity / static-state declarations with NO per-cycle telemetry
//! dependency, so — exactly like the Pi splits them into `bin_rpi::tasks::heartbeat` +
//! `bin_rpi::tasks::state_publisher` — this task owns them on their own cadence:
//!
//! - `DeviceInfo` (#244 shape) — the STOR `power_device` declaration (one internal-metered DC
//!   battery port + NaN-filled `StorageInfo`), mirroring `bin_rpi::tasks::heartbeat::stor_device`.
//! - `Heartbeat` — TMS-required liveness, `{deviceId, sequenceNumber}`, once per second.
//! - `EnergyStartStopState` / `PowerPortState` / `DcLoadSharingState` /
//!   `ActiveMicrogridControllerState` — the four Required STOR state topics (#263), the SAME
//!   PROVISIONAL/static content `bin_rpi::tasks::state_publisher` emits (see that module's doc
//!   comment for the honest-content rationale: `ESSL_OPERATIONAL`, `CS_UNKNOWN`, empty load-sharing,
//!   no active MC).
//!
//! **Transport — BLOCKING send, never a `try_send` drop.** `tx` is a second `Sender` handle onto the
//! SAME `crate::MQTT_CHANNEL` the Seplos poll loop and `boot_i2c::rail_task` already share (an
//! `embassy_sync::channel::Channel` is multi-producer — see `rail_task`'s doc comment). Unlike those
//! two hot/periodic producers (which `try_send` and fail-open on a full channel — a stale 1 s
//! reading isn't worth stalling the poll loop), THIS task uses the async `Sender::send().await`,
//! which applies BACKPRESSURE: it suspends until the broker pump drains a slot, then delivers. That
//! is deliberate — these are slow (60 s) At-Start/PublishLast identity + state topics that MUST each
//! land exactly once; a `try_send` drop of the *last* publish in the burst (the bug this replaces:
//! `ActiveMicrogridControllerState`, the 5th of five, was consistently dropped when the depth-bounded
//! channel momentarily filled under concurrent `emit_reading`/heartbeat/rail load) is unacceptable
//! for them. Backpressure here can only stall THIS task (never the watchdog — that rides
//! `POLL_HEARTBEAT` from the independent Seplos loop), and a stall is correct: if the broker is down
//! nothing is deliverable anyway, and the pending `send` flushes the moment it recovers (better
//! PublishLast latency than waiting up to 60 s for the next refresh). The device-hosted broker's
//! `LatestStore` is the MCU's "retained / PublishLast" mechanism: a late subscriber gets each topic's
//! latest payload from the store, so this task's job is only to emit each topic on cadence (At Start
//! + a slow refresh), not to set an MQTT retain flag itself.
//!
//! **Cadence.** `Heartbeat` every second; `DeviceInfo` + the four state topics At Start and every
//! [`DEVICE_INFO_REFRESH_SECS`] thereafter — mirroring the Pi's 1 s heartbeat + 60 s DeviceInfo /
//! state refresh.
//!
//! **Stack discipline.** The identity structs are large (a `DeviceInfo`'s three `TopicList`s alone
//! are 3 × 16 × 64 B of inline `heapless` storage). Each is built + rendered inside an
//! `#[inline(never)]` helper that returns only the rendered byte count, so the big struct frame is
//! confined to that helper (freed before returning) rather than inflating this task's own poll frame
//! or the shared `[u8; MAX_PAYLOAD]` render scratch — per the repo's zero-copy / frame-isolation
//! rule.

use embassy_time::{Duration, Ticker};

use lib_mapping::render::render_value;
use lib_mapping::schema::{
    topics, ActiveMicrogridControllerState, CableSenseStatus, CircuitWiring, ConfigId,
    DcLoadSharingState, DeviceInfo, DeviceRole, EnergyStartStopLevel, EnergyStartStopState,
    Heartbeat, Identity, LoadSharingInfo, PowerConnectorInfo, PowerConnectorPolarity,
    PowerConnectorType, PowerDeviceInfo, PowerPortDirectionality, PowerPortInfo, PowerPortState,
    PowerPortStatus, PowerSwitchReason, PowerSwitchTransitionCode, ProductInfo, StorageInfo,
    TopicInfo, TopicList, TMS_DATA_MODEL_VERSION,
};
use lib_mapping::sink::{topic_into, MAX_PAYLOAD};
use lib_net::channel::message::{MQTT_PAYLOAD, MQTT_TOPIC};
use lib_net::channel::{MqttUnit, Sender};
use spec_app::RunConfig;

use crate::Rp2350Sender;

/// Heartbeat cadence — TMS-required 1 Hz liveness (mirrors `bin_rpi::tasks::heartbeat`).
const HEARTBEAT_PERIOD_MS: u64 = 1_000;
/// `DeviceInfo` + state-topic re-publish cadence (mirrors the Pi's `DEVICE_INFO_PERIOD` /
/// `STATE_REFRESH_PERIOD`, both 60 s) — belt-and-suspenders durability on top of the broker's
/// latest-value store.
const DEVICE_INFO_REFRESH_SECS: u32 = 60;
/// The single declared power-port number (matches the one `PowerPortInfo` this device advertises).
const DECLARED_PORT: i32 = 0;

/// Product identity fields (the MCU has no product-config source like the Pi's `[product]` TOML —
/// these mirror `bin_rpi::tasks::heartbeat::build_product`'s SHAPE with device-appropriate values;
/// `serial_number` is the per-unit `device_id`, `software_version` the firmware crate version).
const MANUFACTURER: &str = "HVD";
const MODEL: &str = "METEOR";

/// `&str` → fixed-capacity `heapless::String` (truncates on overflow — TMS fields are far under
/// their bounds here, so this never actually truncates). Mirrors `bin_rpi::tasks::util::bounded`.
fn bounded<const N: usize>(v: &str) -> heapless::String<N> {
    let mut o = heapless::String::new();
    let _ = o.push_str(v);
    o
}

fn push_topic(list: &mut TopicList, topic: &str) {
    let _ = list.push(bounded::<64>(topic));
}

/// The STOR `DeviceInfo` — byte-for-byte the shape of `bin_rpi::tasks::heartbeat::stor_device`: the
/// conditional data topic (`DcMeasurementUpdate` + `DcLoadSharingState`), the optional/state topics
/// this device actually publishes, one internal-metered DC battery port, and a NaN-filled
/// `StorageInfo` (bounds not-yet-characterized, per the standard's `@optional float defaults to NaN`
/// convention).
fn build_device_info(device_id: &Identity) -> DeviceInfo {
    let mut published_conditional_topics = TopicList::new();
    push_topic(
        &mut published_conditional_topics,
        topics::DC_MEASUREMENT_UPDATE,
    );
    push_topic(
        &mut published_conditional_topics,
        topics::DC_LOAD_SHARING_STATE,
    );

    let mut published_optional_topics = TopicList::new();
    push_topic(
        &mut published_optional_topics,
        topics::POWER_HARDWARE_UPDATE,
    );
    push_topic(
        &mut published_optional_topics,
        topics::ACTIVE_DIAGNOSTIC_STATE,
    );
    push_topic(&mut published_optional_topics, topics::STORAGE_UPDATE);
    push_topic(
        &mut published_optional_topics,
        topics::ENERGY_START_STOP_STATE,
    );
    push_topic(&mut published_optional_topics, topics::POWER_PORT_STATE);
    push_topic(
        &mut published_optional_topics,
        topics::ACTIVE_MICROGRID_CONTROLLER_STATE,
    );

    let topics_info = TopicInfo {
        data_model_version: bounded(TMS_DATA_MODEL_VERSION),
        published_conditional_topics,
        published_optional_topics,
        supported_request_topics: TopicList::new(),
        extension_topics: None,
    };

    let mut power_ports = heapless::Vec::new();
    let _ = power_ports.push(PowerPortInfo {
        port_number: DECLARED_PORT,
        wiring: CircuitWiring::Dc,
        directionality: PowerPortDirectionality::InOut,
        has_switch: false,
        // The Pi's STOR port sets `has_external_meter: false` (internal metering only).
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
            features: heapless::Vec::new(),
            physical_type: PowerConnectorType::Unknown,
            polarity: PowerConnectorPolarity::Unknown,
        },
        switch_info: None,
    });

    let storage = StorageInfo {
        features: heapless::Vec::new(),
        max_charge_energy: f32::NAN,
        load_sharing: LoadSharingInfo {
            port_number: DECLARED_PORT,
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
        supported_energy_start_stop_levels: heapless::Vec::new(),
    };

    DeviceInfo {
        device_id: device_id.clone(),
        role: DeviceRole::Storage,
        product: ProductInfo {
            nsn: heapless::String::new(),
            gtin: heapless::String::new(),
            manufacturer_name: bounded(MANUFACTURER),
            model_name: bounded(MODEL),
            model_number: heapless::String::new(),
            serial_number: device_id.clone(),
            software_version: bounded(env!("CARGO_PKG_VERSION")),
            platform_id: None,
        },
        topics: topics_info,
        control_hardware: None,
        power_hardware: None,
        power_device: Some(PowerDeviceInfo {
            power_ports,
            grounds: heapless::Vec::new(),
            conversion: None,
            distribution: None,
            source: None,
            storage: Some(storage),
            load: None,
        }),
        control_parameters: None,
        metric_parameters: None,
        control_service: None,
    }
}

/// `EnergyStartStopState` for a healthy, running STOR device — the SAME PROVISIONAL content the Pi's
/// `state_publisher::energy_start_stop_state` emits: `present=future=OPERATIONAL` (a running-state
/// proxy), `request_lock=false`, `elapsed_time=0`, `remaining_time=NaN`, reason `STARTUP`, no actor,
/// future reason `NONE`.
fn build_energy_start_stop(device_id: &Identity) -> EnergyStartStopState {
    EnergyStartStopState {
        device_id: device_id.clone(),
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

/// `PowerPortState` — one `PowerPortStatus` for the single declared port, no cable/connection/switch
/// sensing (mirrors the Pi's `state_publisher::power_port_state`).
fn build_power_port_state(device_id: &Identity) -> PowerPortState {
    let mut power_ports = heapless::Vec::new();
    let _ = power_ports.push(PowerPortStatus {
        port_number: DECLARED_PORT,
        cable_status: CableSenseStatus::Unknown,
        cable_id: None,
        connection: None,
        switch_status: None,
    });
    PowerPortState {
        device_id: device_id.clone(),
        power_ports,
    }
}

/// `DcLoadSharingState` — a non-load-sharing battery: empty `parameters`, `CONFIG_UNKNOWN` (mirrors
/// the Pi's `state_publisher::dc_load_sharing_state`).
fn build_dc_load_sharing(device_id: &Identity) -> DcLoadSharingState {
    DcLoadSharingState {
        device_id: device_id.clone(),
        config: ConfigId::Unknown,
        parameters: heapless::Vec::new(),
    }
}

/// `ActiveMicrogridControllerState` — standalone (no MC selected). The Pi feeds `master_id` from an
/// MC-selection state machine; the MCU has no such machine, so it is honestly `None` here.
fn build_active_mc(device_id: &Identity) -> ActiveMicrogridControllerState {
    ActiveMicrogridControllerState {
        device_id: device_id.clone(),
        master_id: None,
    }
}

// The render helpers below are each `#[inline(never)]` so the (large) identity-struct frame is
// confined to the helper and freed before returning — see the module doc comment's stack note.

#[inline(never)]
fn render_device_info(device_id: &Identity, out: &mut [u8]) -> Option<usize> {
    render_value(&build_device_info(device_id), out).ok()
}
#[inline(never)]
fn render_energy_start_stop(device_id: &Identity, out: &mut [u8]) -> Option<usize> {
    render_value(&build_energy_start_stop(device_id), out).ok()
}
#[inline(never)]
fn render_power_port_state(device_id: &Identity, out: &mut [u8]) -> Option<usize> {
    render_value(&build_power_port_state(device_id), out).ok()
}
#[inline(never)]
fn render_dc_load_sharing(device_id: &Identity, out: &mut [u8]) -> Option<usize> {
    render_value(&build_dc_load_sharing(device_id), out).ok()
}
#[inline(never)]
fn render_active_mc(device_id: &Identity, out: &mut [u8]) -> Option<usize> {
    render_value(&build_active_mc(device_id), out).ok()
}
#[inline(never)]
fn render_heartbeat(device_id: &Identity, sequence_number: u32, out: &mut [u8]) -> Option<usize> {
    let hb = Heartbeat {
        device_id: device_id.clone(),
        sequence_number,
    };
    render_value(&hb, out).ok()
}

/// Render `topic`'s payload into `payload_buf` via `render`, build the full topic
/// (`{prefix}/{device_id}/{slug}`), wrap both in an [`MqttUnit`] (QoS 1), and BLOCK on
/// `tx.send(...).await` until the broker pump makes room — backpressure, never a `try_send` drop
/// (see the module doc comment's Transport note for why the slow state topics use blocking send).
/// A topic that won't build or a payload that won't render is skipped (the next refresh re-asserts
/// it) — the only non-delivery path, and it cannot happen for these fixed, in-budget topics.
async fn send_topic(
    tx: &mut Rp2350Sender,
    prefix: &str,
    device_id: &Identity,
    topic: &str,
    payload_buf: &mut [u8],
    render: impl FnOnce(&mut [u8]) -> Option<usize>,
) {
    let mut full_topic = heapless::String::<MQTT_TOPIC>::new();
    if topic_into(prefix, device_id, topic, &mut full_topic).is_err() {
        return;
    }
    let Some(n) = render(payload_buf) else {
        return;
    };
    let Ok(payload) = heapless::Vec::<u8, MQTT_PAYLOAD>::from_slice(&payload_buf[..n]) else {
        return;
    };
    let _ = tx
        .send(MqttUnit {
            topic: full_topic,
            payload,
            qos: 1,
        })
        .await;
}

/// Publish `DeviceInfo` + the four Required STOR state topics, each via a BLOCKING
/// [`send_topic`] — so every one lands regardless of momentary channel fullness (the fix for
/// `ActiveMicrogridControllerState`, the 5th of five, being dropped on HW; see the module doc's
/// Transport note).
async fn publish_device_and_state(
    tx: &mut Rp2350Sender,
    prefix: &str,
    device_id: &Identity,
    payload_buf: &mut [u8],
) {
    send_topic(
        tx,
        prefix,
        device_id,
        topics::DEVICE_INFO,
        payload_buf,
        |buf| render_device_info(device_id, buf),
    )
    .await;
    send_topic(
        tx,
        prefix,
        device_id,
        topics::ENERGY_START_STOP_STATE,
        payload_buf,
        |buf| render_energy_start_stop(device_id, buf),
    )
    .await;
    send_topic(
        tx,
        prefix,
        device_id,
        topics::POWER_PORT_STATE,
        payload_buf,
        |buf| render_power_port_state(device_id, buf),
    )
    .await;
    send_topic(
        tx,
        prefix,
        device_id,
        topics::DC_LOAD_SHARING_STATE,
        payload_buf,
        |buf| render_dc_load_sharing(device_id, buf),
    )
    .await;
    send_topic(
        tx,
        prefix,
        device_id,
        topics::ACTIVE_MICROGRID_CONTROLLER_STATE,
        payload_buf,
        |buf| render_active_mc(device_id, buf),
    )
    .await;
}

/// Publish one `Heartbeat` via a BLOCKING [`send_topic`].
async fn publish_heartbeat(
    tx: &mut Rp2350Sender,
    prefix: &str,
    device_id: &Identity,
    sequence_number: u32,
    payload_buf: &mut [u8],
) {
    send_topic(
        tx,
        prefix,
        device_id,
        topics::HEARTBEAT,
        payload_buf,
        |buf| render_heartbeat(device_id, sequence_number, buf),
    )
    .await;
}

/// TMS device-identity + Required-STOR-state emitter task. Never returns. Publishes `Heartbeat`
/// every second and `DeviceInfo` + the four state topics At Start and every
/// [`DEVICE_INFO_REFRESH_SECS`]. `tx` is a second sender handle onto `crate::MQTT_CHANNEL` (used with
/// BLOCKING `send` — see the module doc's Transport note); `payload_buf` is this task's own reused
/// render scratch (`static`, never a per-call stack local — see `main.rs`'s `STATE_PAYLOAD_SCRATCH`).
#[embassy_executor::task]
pub(crate) async fn state_task(
    mut tx: Rp2350Sender,
    cfg: RunConfig,
    payload_buf: &'static mut [u8; MAX_PAYLOAD],
) -> ! {
    let device_id: Identity = cfg.device_id.clone();
    let prefix = cfg.prefix.clone();

    // At Start: full device-identity + state set, plus the first heartbeat.
    publish_device_and_state(&mut tx, &prefix, &device_id, &mut payload_buf[..]).await;
    let mut sequence_number: u32 = 0;
    publish_heartbeat(
        &mut tx,
        &prefix,
        &device_id,
        sequence_number,
        &mut payload_buf[..],
    )
    .await;
    sequence_number = sequence_number.wrapping_add(1);

    let mut ticker = Ticker::every(Duration::from_millis(HEARTBEAT_PERIOD_MS));
    let mut seconds: u32 = 0;
    loop {
        ticker.next().await;
        publish_heartbeat(
            &mut tx,
            &prefix,
            &device_id,
            sequence_number,
            &mut payload_buf[..],
        )
        .await;
        sequence_number = sequence_number.wrapping_add(1);

        seconds = seconds.wrapping_add(1);
        if seconds.is_multiple_of(DEVICE_INFO_REFRESH_SECS) {
            publish_device_and_state(&mut tx, &prefix, &device_id, &mut payload_buf[..]).await;
        }
    }
}

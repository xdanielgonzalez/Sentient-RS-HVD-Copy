//! TMS-required 1 s heartbeat (`{deviceId, sequenceNumber}`) — see the gap analysis's "Heartbeat
//! topic" finding. The systemd watchdog covers process liveness but not on-bus liveness. Also owns
//! publishing `DeviceInfo` (#244): both are periodic, device-identity-only publishes with no
//! per-cycle telemetry dependency.
//!
//! **Per identity (D13):** `crate::runtime::supervise` spawns ONE `run_heartbeat` PER derived
//! [`IdentityCfg`] (one per bus), with all inputs INJECTED — the identity (device_id + role),
//! the shared [`ProductParams`], and the broker host:port — so the task no longer re-loads
//! [`crate::config::Config`] (the pre-D13 shape did). It opens its own loopback `rumqttc` client
//! onto the embedded broker (identity-suffixed client-id `{device_id}-heartbeat`, so N identities
//! never collide) — the documented fallback (see `lib_mqtt::broker::impl_broker_subscriber`'s doc
//! comment for why the in-process `LinkRx` path isn't wired here).
//!
//! `DeviceInfo` is shaped BY ROLE: a `ROLE_STORAGE` (battery/Seplos) identity emits the STOR
//! `power_device` (as Wave 1); a `ROLE_CONVERSION` (inverter/Voltronic) identity emits a
//! best-effort, NaN-filled CONV `power_device` (PROVISIONAL — no real inverter characterization).

use std::time::Duration;

use lib_mapping::schema::{
    topics, CircuitWiring, ConversionFeature, ConversionInfo, DeviceInfo, DeviceRole, Heartbeat,
    LoadSharingInfo, PowerConnectorInfo, PowerConnectorPolarity, PowerConnectorType,
    PowerDeviceInfo, PowerPortDirectionality, PowerPortInfo, PowerPortNumber, ProductInfo,
    StorageInfo, TopicInfo, TopicList, TMS_DATA_MODEL_VERSION,
};
use rumqttc::{AsyncClient, MqttOptions, QoS};
use tokio::time::MissedTickBehavior;

use crate::config::{IdentityCfg, ProductParams};
use crate::tasks::util::{bounded, publish_json, publish_json_opts};

/// MQTT keep-alive for this task's own client connection.
const KEEP_ALIVE_SECS: u64 = 30;
/// TMS-required heartbeat cadence.
const HEARTBEAT_PERIOD: Duration = Duration::from_secs(1);
/// `DeviceInfo` re-publish cadence. `DeviceInfo`'s spec profile is PublishLast / TRANSIENT_LOCAL /
/// RELIABLE (IDL 28-36; B.5.1 p.98; D.7 p.293): it is now published **retained + QoS AtLeastOnce**
/// over this task's own loopback `rumqttc` TCP client (the retain limitation noted for the embedded
/// broker's in-process `LinkTx` does NOT apply to this client — see `publish_json_opts`), so a late
/// subscriber receives the current `DeviceInfo` immediately from the broker's retained store. This
/// 60 s re-publish is kept as belt-and-suspenders durability (#265, WS-S).
const DEVICE_INFO_PERIOD: Duration = Duration::from_secs(60);

/// Push a topic literal onto a `TopicList` (`heapless::Vec<String<64>, 16>`), truncating over-long
/// names — TMS topic names are far under 64 chars, so this never actually truncates.
fn push_topic(list: &mut TopicList, topic: &str) {
    let _ = list.push(bounded::<64>(topic));
}

/// A NaN-filled `PowerPortInfo` (bounds not-yet-characterized per the standard's "@optional float
/// defaults to NaN" convention, A.9.6.5). `ac` adds NaN min/max frequency (AC-only); a DC port
/// leaves them `None`. External + internal metering, no switch — PROVISIONAL descriptive metadata.
fn nan_power_port(
    port_number: PowerPortNumber,
    wiring: CircuitWiring,
    directionality: PowerPortDirectionality,
    ac: bool,
) -> PowerPortInfo {
    PowerPortInfo {
        port_number,
        wiring,
        directionality,
        has_switch: false,
        has_external_meter: true,
        has_internal_meter: true,
        has_summary_measurement_update: false,
        min_amperage: f32::NAN,
        max_amperage: f32::NAN,
        short_circuit_amperage: f32::NAN,
        min_voltage: f32::NAN,
        max_voltage: f32::NAN,
        min_frequency: if ac { Some(f32::NAN) } else { None },
        max_frequency: if ac { Some(f32::NAN) } else { None },
        connector_info: PowerConnectorInfo {
            features: heapless::Vec::new(),
            physical_type: PowerConnectorType::Unknown,
            polarity: PowerConnectorPolarity::Unknown,
        },
        switch_info: None,
    }
}

/// The `ProductInfo` for a device, shared across every identity (one product per unit).
fn build_product(product: &ProductParams) -> ProductInfo {
    ProductInfo {
        // NSN/GTIN are not configured for this adapter; emit empty fixed-length strings.
        nsn: heapless::String::new(),
        gtin: heapless::String::new(),
        manufacturer_name: bounded(&product.manufacturer),
        model_name: bounded(&product.model),
        model_number: heapless::String::new(),
        serial_number: bounded(&product.serial_number),
        software_version: bounded(&product.firmware_version),
        platform_id: None,
    }
}

/// The STOR (battery/Seplos) `TopicInfo` + `PowerDeviceInfo` (the Wave 1 shape): DcMeasurementUpdate
/// is the conditional data topic; the rest are optional; one internal-metered DC battery port plus a
/// NaN-filled `StorageInfo`.
fn stor_device(device_id: &heapless::String<32>, product: ProductInfo) -> DeviceInfo {
    let mut published_conditional_topics = TopicList::new();
    push_topic(
        &mut published_conditional_topics,
        topics::DC_MEASUREMENT_UPDATE,
    );
    // DcLoadSharingState is Conditional-Required on HAS DC PORTS (the battery declares a DC port) —
    // see `crate::tasks::state_publisher` (#263).
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
    // The Required STOR state topics this identity now publishes (#263) — declared so the
    // conformance surface (A.8.7: "participate in topics consistent with their DeviceInfo") matches
    // what `crate::tasks::state_publisher` actually emits.
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
        has_external_meter: false,
        ..nan_power_port(0, CircuitWiring::Dc, PowerPortDirectionality::InOut, false)
    });

    let storage = StorageInfo {
        features: heapless::Vec::new(),
        max_charge_energy: f32::NAN,
        load_sharing: nan_load_sharing(0),
        supported_energy_start_stop_levels: heapless::Vec::new(),
    };

    DeviceInfo {
        device_id: device_id.clone(),
        role: DeviceRole::Storage,
        product,
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

/// The CONV (inverter/Voltronic) `TopicInfo` + `PowerDeviceInfo` — PROVISIONAL, best-effort,
/// NaN-filled (no real inverter characterization). AcMeasurementUpdate + DcMeasurementUpdate are the
/// conditional data topics; ONE DESCRIPTIVE power port — the AC output (load side, port 0), the
/// inverter's primary output (matching the port-0 assignment in `lib_mapping::transform_inverter`).
/// A bounded subset: two full `PowerPortInfo`s measured 1693 B — over the old 1664 cap, and still a
/// thin margin under the current 2048 `MAX_PAYLOAD` once real-world product strings land on top — the
/// full port set (grid/PV/battery) is fully described by the AC/DC `MeasurementUpdate` telemetry
/// itself. A `ConversionInfo` marks the active-conversion feature.
fn conv_device(device_id: &heapless::String<32>, product: ProductInfo) -> DeviceInfo {
    let mut published_conditional_topics = TopicList::new();
    push_topic(
        &mut published_conditional_topics,
        topics::AC_MEASUREMENT_UPDATE,
    );
    push_topic(
        &mut published_conditional_topics,
        topics::DC_MEASUREMENT_UPDATE,
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
    // The state topics a CONV device publishes (#263): PowerPortState + AMCState (EnergyStartStop /
    // DcLoadSharing are STOR-only — see `crate::tasks::state_publisher`).
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

    // One descriptive port (PROVISIONAL, bounded subset — see the doc comment): the AC output (0),
    // the inverter's primary load-side output.
    let mut power_ports = heapless::Vec::new();
    let _ = power_ports.push(nan_power_port(
        0,
        CircuitWiring::AcSingle,
        PowerPortDirectionality::InOut,
        true,
    ));

    let mut features = heapless::Vec::new();
    let _ = features.push(ConversionFeature::Active);
    let conversion = ConversionInfo {
        features,
        // Port-level active/passive regulation is not characterized (PROVISIONAL) — both @optional
        // members stay None rather than inventing per-port tap/regulation values.
        active_conversion: None,
        passive_conversion: None,
    };

    DeviceInfo {
        device_id: device_id.clone(),
        role: DeviceRole::Conversion,
        product,
        topics: topics_info,
        control_hardware: None,
        power_hardware: None,
        power_device: Some(PowerDeviceInfo {
            power_ports,
            grounds: heapless::Vec::new(),
            conversion: Some(conversion),
            distribution: None,
            source: None,
            storage: None,
            load: None,
        }),
        control_parameters: None,
        metric_parameters: None,
        control_service: None,
    }
}

/// A NaN-filled `LoadSharingInfo` (values unknown/not-yet-measured).
fn nan_load_sharing(port_number: PowerPortNumber) -> LoadSharingInfo {
    LoadSharingInfo {
        port_number,
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
    }
}

/// Build this identity's `DeviceInfo` (D13): role/topics/power_device shaped by `identity.role`
/// (STOR vs CONV), with the shared `product` fields. Any non-Conversion role (only `Storage` is ever
/// derived) takes the STOR shape.
fn build_device_info(identity: &IdentityCfg, product: &ProductParams) -> DeviceInfo {
    let device_id: heapless::String<32> = bounded(&identity.device_id);
    let product = build_product(product);
    match identity.role {
        DeviceRole::Conversion => conv_device(&device_id, product),
        _ => stor_device(&device_id, product),
    }
}

/// Publish a `Heartbeat` every second (until `shutdown` reports true), plus `DeviceInfo` at startup
/// and every 60 s thereafter, for ONE injected identity (D13).
pub async fn run_heartbeat(
    identity: IdentityCfg,
    product: ProductParams,
    broker_host: String,
    broker_port: u16,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    let device_id: heapless::String<32> = bounded(&identity.device_id);
    let mut opts = MqttOptions::new(format!("{device_id}-heartbeat"), broker_host, broker_port);
    opts.set_keep_alive(Duration::from_secs(KEEP_ALIVE_SECS));
    let (client, mut eventloop) = AsyncClient::new(opts, 10);

    let device_info = build_device_info(&identity, &product);
    // Publish once at startup immediately, retained + QoS AtLeastOnce (DeviceInfo's PublishLast /
    // TRANSIENT_LOCAL / RELIABLE profile — see `DEVICE_INFO_PERIOD`), before the periodic interval
    // below (which fires its own first tick right away too — consumed by `info_interval.tick()`).
    publish_json_opts(
        &client,
        &device_id,
        topics::DEVICE_INFO,
        &device_info,
        QoS::AtLeastOnce,
        true,
    );

    let mut hb_interval = tokio::time::interval(HEARTBEAT_PERIOD);
    hb_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut info_interval = tokio::time::interval(DEVICE_INFO_PERIOD);
    info_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
    // `tokio::time::interval` fires its first tick immediately; consume it here since the startup
    // publish above already covers it.
    info_interval.tick().await;

    let mut sequence_number: u32 = 0;

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    break;
                }
            }
            _ = hb_interval.tick() => {
                let hb = Heartbeat {
                    device_id: device_id.clone(),
                    sequence_number,
                };
                publish_json(&client, &device_id, topics::HEARTBEAT, &hb);
                sequence_number = sequence_number.wrapping_add(1);
            }
            _ = info_interval.tick() => {
                // Retained + QoS1 refresh (durability belt-and-suspenders — see `DEVICE_INFO_PERIOD`).
                publish_json_opts(
                    &client,
                    &device_id,
                    topics::DEVICE_INFO,
                    &device_info,
                    QoS::AtLeastOnce,
                    true,
                );
            }
            event = eventloop.poll() => {
                // The eventloop must be polled continuously for `try_publish`'d packets to actually
                // flush (rumqttc queues them internally); errors just mean a transient disconnect —
                // rumqttc reconnects on the next poll, so log and keep looping.
                if let Err(e) = event {
                    log::debug!("heartbeat: mqtt eventloop error (will retry): {e}");
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use lib_mapping::schema::DeviceRole;

    use super::{build_device_info, IdentityCfg, ProductParams};
    use crate::config::ProtocolCfg;

    fn product() -> ProductParams {
        ProductParams {
            manufacturer: "HVD".to_string(),
            model: "METEOR".to_string(),
            serial_number: "SN-0001".to_string(),
            firmware_version: "0.3.0".to_string(),
        }
    }

    fn identity(device_id: &str, role: DeviceRole, protocol: ProtocolCfg) -> IdentityCfg {
        IdentityCfg {
            device_id: device_id.to_string(),
            role,
            protocol,
        }
    }

    fn assert_within_budget(info: &lib_mapping::schema::DeviceInfo) {
        let mut buf = [0u8; lib_mapping::sink::MAX_PAYLOAD];
        let n = lib_mapping::render::render_value(info, &mut buf)
            .expect("DeviceInfo must encode within MAX_PAYLOAD");
        assert!(
            n <= lib_mapping::sink::MAX_PAYLOAD,
            "encoded {n} bytes exceeds MAX_PAYLOAD={}",
            lib_mapping::sink::MAX_PAYLOAD
        );
    }

    #[test]
    fn stor_device_info_encodes_within_budget_and_carries_storage() {
        let info = build_device_info(
            &identity("METEOR-001", DeviceRole::Storage, ProtocolCfg::Seplos),
            &product(),
        );
        assert_eq!(info.role, DeviceRole::Storage);
        let pd = info.power_device.as_ref().expect("power_device present");
        assert!(pd.storage.is_some());
        assert!(pd.conversion.is_none());
        assert_within_budget(&info);
    }

    #[test]
    fn conv_device_info_encodes_within_budget_and_carries_conversion() {
        let info = build_device_info(
            &identity(
                "METEOR-001-inv",
                DeviceRole::Conversion,
                ProtocolCfg::Voltronic,
            ),
            &product(),
        );
        assert_eq!(info.role, DeviceRole::Conversion);
        let pd = info.power_device.as_ref().expect("power_device present");
        assert!(pd.conversion.is_some());
        assert!(pd.storage.is_none());
        // One descriptive port (AC output) — bounded subset that fits MAX_PAYLOAD with headroom.
        assert_eq!(pd.power_ports.len(), 1);
        assert_within_budget(&info);
    }

    #[test]
    fn conv_advertises_ac_and_dc_conditional_topics() {
        let info = build_device_info(
            &identity(
                "METEOR-001-inv",
                DeviceRole::Conversion,
                ProtocolCfg::Voltronic,
            ),
            &product(),
        );
        let cond = &info.topics.published_conditional_topics;
        assert!(cond.iter().any(|t| t.as_str() == "AcMeasurementUpdate"));
        assert!(cond.iter().any(|t| t.as_str() == "DcMeasurementUpdate"));
    }
}

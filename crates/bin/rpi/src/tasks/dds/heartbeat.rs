//! DDS-side `Heartbeat` + `DeviceInfo` (#324, WP6) — the DDS transport counterpart of
//! `crate::tasks::heartbeat`. Same 1 s Heartbeat / 60 s DeviceInfo cadence (READ that file's doc
//! comment for the full spec-timing rationale — `DeviceInfo`'s PublishLast/TRANSIENT_LOCAL/RELIABLE
//! profile; `generated_qos::qos_profile` already maps both topics to their D.5 profile, so the
//! per-writer QoS itself carries `TRANSIENT_LOCAL` — a late DDS subscriber gets the current
//! `DeviceInfo` from the writer's own history, no application-level "retain" needed the way MQTT
//! needed one).
//!
//! `DeviceInfo`'s DDS patch type has not landed yet (`@optional`-heavy — see
//! `lib_dds::dustdds::types::patch`'s module doc comment), so this task's `DeviceInfo` enqueue is
//! dropped by `lib_dds::run_dds_stack` today (`DdsError::UnsupportedTopic`, logged, never a crash);
//! `Heartbeat` IS wired and is the runtime smoke test's primary datum.

use std::time::Duration;

use lib_dds::DdsHandle;
use lib_mapping::schema::{
    topics, DeviceInfo, Heartbeat, Identity, ProductInfo, TopicInfo, TopicList,
    TMS_DATA_MODEL_VERSION,
};
use lib_tms::TmsTopicId;
use tokio::time::MissedTickBehavior;

use super::util::render_and_send;
use crate::config::{IdentityCfg, ProductParams};
use crate::tasks::util::bounded;

/// TMS-required heartbeat cadence — matches `tasks::heartbeat::HEARTBEAT_PERIOD`.
const HEARTBEAT_PERIOD: Duration = Duration::from_secs(1);
/// `DeviceInfo` re-publish cadence — matches `tasks::heartbeat::DEVICE_INFO_PERIOD` (belt-and-
/// suspenders durability on top of the writer's own `TRANSIENT_LOCAL` history).
const DEVICE_INFO_PERIOD: Duration = Duration::from_secs(60);

/// A minimal, honest `DeviceInfo` for `identity` — device id/role/product filled in; every
/// `@optional` power/control/metric section left `None` (no DDS-side characterization work done
/// yet, mirrors this task's narrower scope vs. `tasks::heartbeat::build_device_info`'s fuller MQTT
/// shape) plus the one conditional topic this WP actually wires over DDS
/// (`DcMeasurementUpdate`).
fn build_device_info(identity: &IdentityCfg, product: &ProductParams) -> DeviceInfo {
    let device_id: Identity = bounded(&identity.device_id);
    let mut published_conditional_topics = TopicList::new();
    let _ = published_conditional_topics.push(bounded(topics::DC_MEASUREMENT_UPDATE));
    DeviceInfo {
        device_id: device_id.clone(),
        role: identity.role,
        product: ProductInfo {
            nsn: heapless::String::new(),
            gtin: heapless::String::new(),
            manufacturer_name: bounded(&product.manufacturer),
            model_name: bounded(&product.model),
            model_number: heapless::String::new(),
            serial_number: bounded(&product.serial_number),
            software_version: bounded(&product.firmware_version),
            platform_id: None,
        },
        topics: TopicInfo {
            data_model_version: bounded(TMS_DATA_MODEL_VERSION),
            published_conditional_topics,
            published_optional_topics: TopicList::new(),
            supported_request_topics: TopicList::new(),
            extension_topics: None,
        },
        control_hardware: None,
        power_hardware: None,
        control_parameters: None,
        metric_parameters: None,
        control_service: None,
        power_device: None,
    }
}

/// Publish `Heartbeat` every second and `DeviceInfo` at startup + every 60 s, over DDS, for ONE
/// injected identity (D13) — mirrors `tasks::heartbeat::run_heartbeat`'s cadence exactly, on the
/// shared `handle` (the same [`DdsHandle`] `DdsTap` uses).
pub async fn run_dds_heartbeat(
    identity: IdentityCfg,
    product: ProductParams,
    handle: DdsHandle,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    let device_id: Identity = bounded(&identity.device_id);
    let device_info = build_device_info(&identity, &product);

    // At-start DeviceInfo publish, before the periodic interval below (which fires its own first
    // tick immediately — consumed here, mirroring `tasks::heartbeat::run_heartbeat`).
    render_and_send(&handle, &device_id, TmsTopicId::DeviceInfo, &device_info);

    let mut hb_interval = tokio::time::interval(HEARTBEAT_PERIOD);
    hb_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut info_interval = tokio::time::interval(DEVICE_INFO_PERIOD);
    info_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
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
                render_and_send(&handle, &device_id, TmsTopicId::Heartbeat, &hb);
                sequence_number = sequence_number.wrapping_add(1);
            }
            _ = info_interval.tick() => {
                render_and_send(&handle, &device_id, TmsTopicId::DeviceInfo, &device_info);
            }
        }
    }
}

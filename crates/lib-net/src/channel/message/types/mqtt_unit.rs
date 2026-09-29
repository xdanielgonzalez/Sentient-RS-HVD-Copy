#[cfg(feature = "packed")]
use serde::{Deserialize, Serialize};

use crate::channel::message::{MQTT_PAYLOAD, MQTT_TOPIC};

/// **MQTT-dissemination channel.** A publish unit leaving the pipeline: topic + payload + QoS. This
/// is the existing `publish_tms` boundary made a first-class message, so the MQTT stage is an
/// independent consumer.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "packed", derive(Serialize, Deserialize))]
pub struct MqttUnit {
    /// Full topic, e.g. `tms/METEOR-001/dc_measurement_update`.
    pub topic: heapless::String<MQTT_TOPIC>,
    /// Serialized payload (TMS JSON today).
    pub payload: heapless::Vec<u8, MQTT_PAYLOAD>,
    /// MQTT QoS (0/1/2).
    pub qos: u8,
}

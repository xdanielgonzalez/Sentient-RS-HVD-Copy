use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use lib_net::channel::MqttUnit;

use crate::MQTT_CHANNEL_DEPTH;

/// Sender half of the static MQTT-unit channel (`crate::MQTT_CHANNEL` / `crate::SEALED_CHANNEL`),
/// concrete over this platform's mutex + channel depth.
pub(crate) type Rp2350Sender =
    embassy_sync::channel::Sender<'static, CriticalSectionRawMutex, MqttUnit, MQTT_CHANNEL_DEPTH>;

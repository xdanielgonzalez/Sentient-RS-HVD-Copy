use bin_mcu_platform::MeshUnit;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;

use crate::MESH_CHANNEL_DEPTH;

/// Receiver half of the static mesh-radio channel (`crate::MESH_CHANNEL`) — same shape convention
/// as [`super::Rp2350Receiver`] (the MQTT channel's receiver alias), carrying [`MeshUnit`] instead
/// of `MqttUnit`.
pub(crate) type Rp2350MeshReceiver =
    embassy_sync::channel::Receiver<'static, CriticalSectionRawMutex, MeshUnit, MESH_CHANNEL_DEPTH>;

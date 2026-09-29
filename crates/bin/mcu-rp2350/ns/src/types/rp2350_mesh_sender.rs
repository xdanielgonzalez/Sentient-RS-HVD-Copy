use bin_mcu_platform::MeshUnit;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;

use crate::MESH_CHANNEL_DEPTH;

/// Sender half of the static mesh-radio channel (`crate::MESH_CHANNEL`) — same shape convention as
/// [`super::Rp2350Sender`] (the MQTT channel's sender alias), carrying [`MeshUnit`] instead of
/// `MqttUnit`.
pub(crate) type Rp2350MeshSender =
    embassy_sync::channel::Sender<'static, CriticalSectionRawMutex, MeshUnit, MESH_CHANNEL_DEPTH>;

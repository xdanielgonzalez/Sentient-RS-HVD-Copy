#[cfg(feature = "packed")]
use serde::{Deserialize, Serialize};

use super::flow_dir::FlowDir;
use crate::channel::message::WIRE_MTU;

/// **Wire-byte channel.** A raw L2 (Ethernet) frame crossing the network-link boundary
/// (net stack ↔ AF_XDP/Zephyr-NIC/W5500 link). Owned above L2; the link only moves these bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "packed", derive(Serialize, Deserialize))]
pub struct WireFrame {
    /// Ingress (from NIC) or egress (to NIC).
    pub dir: FlowDir,
    /// The raw frame bytes (L2 header … payload), bounded to the MTU.
    pub data: heapless::Vec<u8, WIRE_MTU>,
}

// Private prelude module for device split
pub use crate::channel::message::WIRE_MTU;
pub use crate::channel::{FlowDir, Receiver, Sender, WireFrame};
pub use smoltcp::phy::{self, DeviceCapabilities, Medium};
pub use smoltcp::time::Instant;

pub use super::types::*;

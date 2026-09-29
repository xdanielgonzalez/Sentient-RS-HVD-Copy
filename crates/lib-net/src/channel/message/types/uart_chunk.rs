#[cfg(feature = "packed")]
use serde::{Deserialize, Serialize};

use super::flow_dir::FlowDir;
use crate::channel::message::UART_CHUNK;

/// **UART-byte channel.** A raw chunk of Seplos serial bytes crossing the transport ↔ parser
/// boundary. The transport produces/consumes chunks; the parser owns framing above it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "packed", derive(Serialize, Deserialize))]
pub struct UartChunk {
    /// Ingress (bytes read from the bus) or egress (a request frame to write).
    pub dir: FlowDir,
    /// The raw serial bytes, bounded to one frame's worth.
    pub data: heapless::Vec<u8, UART_CHUNK>,
}

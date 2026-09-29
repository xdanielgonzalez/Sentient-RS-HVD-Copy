#[cfg(feature = "packed")]
use serde::{Deserialize, Serialize};

/// Direction of a byte-stream unit at a duplex boundary (a link/transport is bidirectional).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "packed", derive(Serialize, Deserialize))]
pub enum FlowDir {
    /// Into the pipeline (received from the wire/bus).
    Ingress,
    /// Out of the pipeline (to be transmitted).
    Egress,
}

use super::prelude::*;

impl SimReply {
    /// The bytes to write to the wire, or an empty slice for [`SimReply::Silence`].
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            SimReply::Bytes(b) => b.as_slice(),
            SimReply::Silence => &[],
        }
    }
}

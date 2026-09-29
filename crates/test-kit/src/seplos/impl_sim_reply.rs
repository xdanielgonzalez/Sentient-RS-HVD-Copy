use super::prelude::*;

impl SimReply {
    /// The reply bytes, or an empty slice for [`SimReply::Silence`].
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            SimReply::Bytes(v) => v.as_slice(),
            SimReply::Silence => &[],
        }
    }
}

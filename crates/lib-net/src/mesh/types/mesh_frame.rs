/// The minimal Meshtastic on-air message shape this crate hand-encodes: a `ToRadio` envelope
/// carrying one `MeshPacket` on `channel`, whose payload is a `Data{portnum, payload}` — see
/// `crate::mesh::impl_frame`'s module doc comment for the field-number provenance and the
/// hand-rolled-vs-`prost` decision (supply-chain decision 0006).
#[derive(Debug, Clone, Copy)]
pub struct MeshFrame<'a> {
    /// Meshtastic channel index (`MeshPacket.channel`, field 3).
    pub channel: u32,
    /// Meshtastic `PortNum` the payload is tagged with (`Data.portnum`, field 1).
    pub portnum: u32,
    /// The already-condensed on-air payload bytes (`Data.payload`, field 2) — the output of
    /// [`crate::mesh::condense`].
    pub payload: &'a [u8],
}

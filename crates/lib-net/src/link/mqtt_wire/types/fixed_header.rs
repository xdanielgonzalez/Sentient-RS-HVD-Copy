/// The decoded fixed header (MQTT 3.1.1 section 2.2): packet type, the 4 flag bits, and the
/// Remaining Length — plus how many bytes the header itself occupied, so the caller can slice the
/// variable header/payload that follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FixedHeader {
    /// High nibble of byte 1 (one of the `PKT_*` constants).
    pub packet_type: u8,
    /// Low nibble of byte 1 (DUP/QoS/RETAIN for PUBLISH; fixed per spec for other types).
    pub flags: u8,
    /// The decoded Remaining Length (bytes following the fixed header).
    pub remaining_length: usize,
    /// Total bytes consumed by the fixed header (1 type/flags byte + 1-4 varint bytes).
    pub header_len: usize,
}

/// A parsed, checksum-verified response frame, split into header fields and the raw INFO bytes
/// (INFO already decoded from ASCII-hex pairs into binary).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponseFrame {
    /// Device address (DIP) from the ADR field.
    pub adr: u8,
    /// Return code from the RTN field (`0x00` = success).
    pub rtn: u8,
    /// Declared LENID (INFO hex-char count) from the LENGTH field.
    pub lenid: u16,
    /// INFO payload, decoded from ASCII-hex pairs to raw bytes.
    pub info: heapless::Vec<u8, 256>,
}

/// A parsed CONNACK (MQTT 3.1.1 section 3.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnAck {
    /// Session Present flag (bit 0 of the Connect Acknowledge Flags byte).
    pub session_present: bool,
    /// Connect Return Code (0 = accepted; 1-5 are the spec's refusal codes).
    pub return_code: u8,
}

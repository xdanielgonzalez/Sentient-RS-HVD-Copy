/// Meshtastic radio connection + condensation settings.
#[derive(Debug, Clone)]
pub struct MeshConfig {
    /// Serial device path the Meshtastic radio is attached on (e.g. `/dev/ttyUSB1`).
    pub device_path: String,
    /// Serial baud rate.
    pub baud: u32,
    /// Meshtastic channel index to send on (`MeshPacket.channel`; `0` = the radio's primary
    /// channel).
    pub channel: u32,
    /// Maximum on-air payload size in bytes (the baseline's greedy condensation budget, ~200).
    pub max_payload: usize,
    /// Minimum interval between mesh sends, in milliseconds (LoRa duty-cycle friendliness).
    pub send_interval_ms: u64,
}

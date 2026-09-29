//! The typed boundary messages — one per inter-stage channel.
//!
//! Each message is the *only* thing that crosses its boundary, bounded (heapless, no heap) and —
//! under `feature = "packed"` — serializable so the channel can be an IPC transport between isolated
//! processes, not just an in-process queue. Because every byte that crosses is a typed, bounded
//! value, each boundary is a place we can capture-vector-test the exact bytes on the wire (the same
//! discipline the Seplos frame vectors use).

mod impl_wire_frame;
mod types;

pub use types::*;

/// Max L2 (Ethernet) frame: 1500 MTU + 14 header. The wire-byte boundary unit.
pub const WIRE_MTU: usize = 1514;
/// Max UART read chunk. A Seplos frame is < 256 bytes.
pub const UART_CHUNK: usize = 256;
/// Max application payload handed across the crypto boundary (a TMS JSON payload fits in 8 KiB).
pub const CRYPTO_MAX: usize = 8192;
/// Max MQTT topic length.
pub const MQTT_TOPIC: usize = 128;
/// Max MQTT payload length.
pub const MQTT_PAYLOAD: usize = 8192;

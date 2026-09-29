/// Ceiling on the condensed on-air JSON payload the mesh channel carries between
/// [`super::MeshPlatform`]'s emit seam and the board crate's radio task — Meshtastic's own
/// `Data.payload` ceiling (`meshtastic_Constants_DATA_PAYLOAD_LEN`, verified against
/// `meshtastic/protobufs` @ master; see the "Meshtastic Serial/UART Protobuf Client API (Heltec
/// V4)" research note), which is tighter than the 512-byte on-air stream-frame ceiling
/// (`lib_net::mesh::MAX_RADIO_FRAME`) the radio task applies later when it wraps this payload in a
/// `MeshFrame`.
pub const MESH_PAYLOAD: usize = 233;

/// One condensed mesh-radio frame, queued from [`super::MeshPlatform`]'s emit seam onto the board
/// crate's mesh channel for its radio task to wrap in a Meshtastic `MeshFrame` (+ stream framing)
/// and write over the Heltec UART.
///
/// Mirrors `lib_net::channel::MqttUnit`'s shape (a bounded `heapless` byte buffer carried as a
/// channel message) rather than reaching for `lib_net::mesh::Condensed`: that type is
/// `mesh-tokio`-only (std/`Vec`/alloc — the Pi's host radio loop), while `lib_net::mesh`'s bare
/// `mesh` feature (no_std, no alloc — what this crate and the MCU board crate actually enable) has
/// no channel-message type of its own. This is the no_std equivalent, scoped to this crate since
/// that gap sits outside WP3's file ownership (`crates/lib-net/src/mesh/**` is not in this work
/// package's file list).
#[derive(Debug, Clone)]
pub struct MeshUnit {
    /// The condensed JSON payload — `lib_net::mesh::condense`'s output, NOT yet wrapped in
    /// Meshtastic's on-air `MeshFrame`/stream framing (the radio task's job, not this seam's).
    pub payload: heapless::Vec<u8, MESH_PAYLOAD>,
}

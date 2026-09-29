use lib_net::mesh::MAX_RADIO_FRAME;

use super::rp2350_mesh_parser_state::FromRadioState;

/// Resyncing `FromRadio` reader over the Meshtastic serial stream framing (`0x94 0xC3` magic + a
/// big-endian `u16` length + the raw protobuf message — see `lib_net::mesh::impl_frame`'s module
/// doc comment for the on-air shape this mirrors on receive, and the "Meshtastic Serial/UART
/// Protobuf Client API (Heltec V4)" research note for the firmware receiver behavior this is
/// verified against).
///
/// **Why not `SerialTransport::read_until`.** That primitive is shaped for Seplos's
/// `\r`-terminated ASCII exchange. Meshtastic framing is binary with no delimiter, and the firmware
/// legally interleaves non-frame "debug output" bytes between frames — a reader must resync on the
/// magic itself rather than search for a terminator, which is exactly what this state machine does,
/// fed raw bytes from a plain `SerialTransport::read` (see `boot_mesh.rs::mesh_radio_task`).
///
/// **Minimum viable (WP3).** Recognizes complete `FromRadio` frame boundaries and hands the
/// payload bytes back to the caller via [`FromRadioParser::feed`]'s callback; it does NOT decode
/// the protobuf `oneof` inside — `QueueStatus`/`ClientNotification`/etc. decoding (surfacing
/// delivery backpressure/errors) is future work, not this round's scope (see the research note's
/// "stretch" list). A declared length over [`MAX_RADIO_FRAME`] is treated as corruption exactly
/// like the firmware's own receiver (`if (len > MAX_TO_FROM_RADIO_SIZE) rxPtr = 0;`) and resets the
/// search for the magic.
pub(crate) struct FromRadioParser {
    pub(crate) state: FromRadioState,
    /// Accumulates the current frame's payload bytes. Sized to [`MAX_RADIO_FRAME`] (the firmware's
    /// own `MAX_TO_FROM_RADIO_SIZE`) — the same ceiling `lib_net::mesh`'s encoder applies on send.
    pub(crate) buf: [u8; MAX_RADIO_FRAME],
    /// Bytes of the current frame's payload accumulated so far.
    pub(crate) len: usize,
    /// The declared payload length (from the 2-byte header), once known.
    pub(crate) declared_len: usize,
}

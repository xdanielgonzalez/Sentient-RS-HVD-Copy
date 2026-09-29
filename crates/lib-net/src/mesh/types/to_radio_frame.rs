/// Convert condensed telemetry bytes into the on-air Meshtastic frame shape (transport framing —
/// distinct from [`crate::mesh::condense`]'s field-selection concern).
pub trait ToRadioFrame {
    /// Write the framed bytes into `out`, returning the number of bytes written.
    fn to_radio_frame(&self, out: &mut [u8]) -> usize;
}

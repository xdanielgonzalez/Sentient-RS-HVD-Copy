/// Byte-oriented resync state machine's position within the Meshtastic `0x94 0xC3` stream framing
/// — see [`super::rp2350_mesh_parser::FromRadioParser`]'s doc comment for the framing shape this
/// walks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FromRadioState {
    /// Hunting for `START1` (`0x94`); any other byte is Meshtastic's legally-interleaved "debug
    /// output" and is simply discarded (meshtastic.org's Client API doc: "While looking for START1
    /// any other characters are printed as debug output").
    Magic0,
    /// Saw `START1`; hunting for `START2` (`0xC3`). Any OTHER byte drops back to [`Self::Magic0`],
    /// except another `0x94` (covers `0x94 0x94 0xC3` — the second `0x94` might itself be the real
    /// START1), which stays in this state.
    Magic1,
    /// Reading the big-endian `u16` declared length's high byte.
    LenHi,
    /// Reading the low byte; carries the high byte already read.
    LenLo(u8),
    /// Accumulating `declared_len` payload bytes into `FromRadioParser::buf`.
    Payload,
}

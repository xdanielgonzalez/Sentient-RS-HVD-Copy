/// How [`super::seplos_sim::SeplosSim::respond`] answers the next request. Set via [`super::seplos_sim::SeplosSim::set_behavior`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Behavior {
    /// Reply with a byte-exact, checksum-valid response synthesized from the current
    /// [`super::battery_state::BatteryState`].
    Normal,
    /// Reply with a well-formed frame whose checksum no longer matches (one INFO byte flipped
    /// after a valid frame was built).
    CorruptChecksum,
    /// Reply with a well-formed, checksum-**valid** frame addressed to a different DIP than the
    /// one polled — a real device answering for someone else, not a corrupted frame.
    WrongAddress,
    /// Reply with a well-formed, checksum-**valid** frame carrying the given non-`0x00` RTN code
    /// and an empty INFO payload — how a real device reports a protocol-level error (`02H`
    /// CHKSUM, `03H` LCHKSUM, `04H` invalid CID2, ...) rather than staying silent. Persists across
    /// calls until [`super::seplos_sim::SeplosSim::set_behavior`] changes it again, like every other variant here.
    RtnError(u8),
    /// Reply with only the first `usize` bytes of the well-formed frame, then nothing (an
    /// unterminated partial frame — never reaches the `\r` delimiter).
    Truncated(usize),
    /// Never reply (a silent/non-responding bus).
    Silence,
    /// Reply with non-frame bytes terminated by `\r` (garbage that still satisfies a delimiter
    /// read).
    Garbage,
}

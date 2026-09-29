use super::prelude::*;

impl Command {
    /// The two ASCII-hex CID2 characters for this command (e.g. `Telemetry` → `b"42"`).
    pub fn cid2(self) -> [u8; 2] {
        match self {
            Command::Telemetry => *b"42",
            Command::Fault => *b"44",
            Command::SerialNumber => *b"A4",
        }
    }
}

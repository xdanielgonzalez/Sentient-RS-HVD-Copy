use super::prelude::*;

impl fmt::Display for SeplosError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SeplosError::BadSoi => write!(f, "frame did not start with SOI '~'"),
            SeplosError::MissingEoi => write!(f, "frame did not end with EOI '\\r'"),
            SeplosError::Truncated => write!(f, "frame shorter than minimum length"),
            SeplosError::NonHex => write!(f, "non-hex byte in ASCII-hex field"),
            SeplosError::NonAscii => write!(f, "non-ASCII byte in text field"),
            SeplosError::BadChecksum { expected, got } => {
                write!(
                    f,
                    "checksum mismatch: expected {expected:04X}, got {got:04X}"
                )
            }
            SeplosError::BadLengthChecksum => write!(f, "LENGTH LCHKSUM nibble invalid"),
            SeplosError::LengthMismatch { lenid, actual } => {
                write!(f, "LENID {lenid} != actual INFO length {actual}")
            }
            SeplosError::RtnError(code) => write!(f, "response RTN error code {code:02X}"),
            SeplosError::AddrMismatch { expected, got } => {
                write!(f, "address mismatch: expected {expected}, got {got}")
            }
            SeplosError::InfoUnderrun => write!(f, "INFO payload too short for field layout"),
            SeplosError::TooManyElements => write!(f, "element count exceeds bounded capacity"),
            SeplosError::AddrOutOfRange(a) => write!(f, "DIP address {a} out of range 0..=15"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for SeplosError {}

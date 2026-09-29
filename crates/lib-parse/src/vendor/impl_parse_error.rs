use super::prelude::*;

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::BadFraming => write!(f, "frame did not match expected framing"),
            ParseError::BadChecksum => write!(f, "frame integrity check failed"),
            ParseError::Malformed => write!(f, "payload malformed for declared layout"),
            ParseError::DeviceError(c) => write!(f, "device error return code {c:#04x}"),
            ParseError::AddressMismatch => write!(f, "response address mismatch"),
            ParseError::CapacityExceeded => write!(f, "output capacity exceeded"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for ParseError {}

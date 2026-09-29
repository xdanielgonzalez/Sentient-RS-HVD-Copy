use super::prelude::*;

impl core::fmt::Display for OpenError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            OpenError::InvalidParams(p) => write!(f, "invalid serial parameter: {p}"),
            OpenError::Io(e) => write!(f, "{e}"),
            OpenError::Direction(msg) => write!(f, "RS-485 direction control: {msg}"),
        }
    }
}

impl std::error::Error for OpenError {}

use super::prelude::*;
use core::fmt;

impl From<SecurityError> for TlsError {
    /// Carry a seam refusal up without flattening it. A custody refusal, an unapproved algorithm
    /// and a failed verification are different findings and stay legible as different findings —
    /// but they all land on the error side, which is the only property that matters for safety.
    fn from(err: SecurityError) -> Self {
        TlsError::Security(err)
    }
}

impl fmt::Display for TlsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TlsError::Unimplemented => {
                f.write_str("TLS operation is unimplemented (ADR 0013 WP-3 skeleton) — refused")
            }
            TlsError::Handshake => f.write_str("TLS handshake failed"),
            TlsError::Protocol => f.write_str("TLS record failed to decrypt or authenticate"),
            TlsError::BufferTooSmall => f.write_str("buffer too small for the TLS operation"),
            TlsError::NotAuthorized => {
                f.write_str("authenticated peer is not authorized for this action")
            }
            TlsError::Security(err) => write!(f, "security seam refused: {err}"),
        }
    }
}

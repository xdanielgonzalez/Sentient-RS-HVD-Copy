use super::tls_role::TlsRole;
use rustls::Connection;

/// Drives a rustls client or server connection over a byte stream. Owns no socket: the caller feeds
/// it raw bytes read from the TCP socket and drains bytes to write back to it every poll.
pub struct TlsStage {
    pub(crate) role: TlsRole,
    pub(crate) conn: Connection,
}

/// Which side of the handshake a [`crate::link::tls::TlsStage`] plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlsRole {
    /// Initiates the handshake (sends ClientHello), verifies the server's certificate.
    Client,
    /// Accepts the handshake (sends ServerHello + certificate).
    Server,
}

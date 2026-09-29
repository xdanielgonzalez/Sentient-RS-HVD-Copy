/// A TLS-stage failure. Fail-closed: any variant means the record/handshake state is untrusted and
/// the connection must not be treated as secure.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlsError {
    /// The handshake failed (bad certificate, protocol mismatch, alert received).
    Handshake,
    /// A record failed to decrypt / authenticate.
    Protocol,
    /// The caller-provided buffer was too small to drain/write into.
    BufferTooSmall,
}

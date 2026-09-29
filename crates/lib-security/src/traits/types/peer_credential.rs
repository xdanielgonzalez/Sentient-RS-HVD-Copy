/// A credential a peer presented, borrowed from the caller's receive buffer — never copied, never
/// allocated. It is **untrusted, attacker-controlled input** until
/// [`verify_peer`](super::trust_store::TrustStore::verify_peer) returns `Ok`.
///
/// Borrowing rather than owning is what keeps this `no_std` + no-`alloc`: a TLS handshake already
/// holds the peer's certificate message in a buffer, and the trust store parses out of that buffer
/// in place. The lifetime therefore ties a `PeerCredential` to the record it came from, which is
/// also the right scope — the credential is meaningless after the handshake buffer is recycled.
///
/// See ADR `docs/decisions/0013-transport-security-trust-store.md` §1.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub enum PeerCredential<'a> {
    /// An RFC 7250 raw public key: a bare DER `SubjectPublicKeyInfo`, no certificate wrapper.
    RawPublicKey {
        /// DER-encoded `SubjectPublicKeyInfo`.
        spki: &'a [u8],
    },
    /// An X.509 credential: the end-entity certificate plus whatever intermediates the peer chose
    /// to send. Under [`TrustModel::PinnedSpki`](super::trust_model::TrustModel::PinnedSpki) the
    /// intermediates are **ignored** — only the end-entity's `SubjectPublicKeyInfo` is extracted
    /// and matched — and an implementation must not be tricked into treating a peer-supplied
    /// intermediate as an anchor.
    X509Chain {
        /// DER-encoded end-entity certificate.
        end_entity: &'a [u8],
        /// DER-encoded intermediates, in the order the peer sent them. Peer-controlled: never an
        /// anchor, never trusted, and an implementation may ignore them entirely.
        intermediates: &'a [&'a [u8]],
    },
}

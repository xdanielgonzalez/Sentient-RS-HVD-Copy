use super::secure_provider::SecureProvider;

/// Client-certificate verification by **pinned SPKI**, not PKI — a skeleton (ADR 0013 §3, WP-3).
///
/// # What it will be
///
/// The implementation of rustls's `server::danger::ClientCertVerifier`. The client sends an ordinary
/// X.509 certificate — so stock tooling (mosquitto, paho) works unchanged — and this verifier
/// extracts its `SubjectPublicKeyInfo`, hashes it, and matches the hash against the short list of
/// pinned SPKI hashes held by the `TrustStore` behind the seam. The resolved `AnchorId` is what
/// authorization is later keyed on (ADR 0013 §6, WP-7).
///
/// **No chain construction. No `notBefore` / `notAfter`. No webpki.** That is not a simplification
/// taken for convenience: it is what lets a device with no real-time clock authenticate a peer at
/// all (ADR 0013 §5). `TrustStore::verify_peer` is called with `now: None` and a `PinnedSpki` model
/// succeeds, because pinning has no validity window to check. A future `X509Chain` model **must**
/// fail closed on `None`, and the trait signature makes that a compile-visible obligation rather
/// than a comment.
///
/// # What it costs, stated so nobody infers otherwise
///
/// This is pinning, not PKI: **no CRL, no OCSP, no expiry**. Rotation is a signed configuration
/// push and that is the entire revocation mechanism. For a broker with three known clients that is
/// the right trade; ADR 0013's revisit triggers name "client count grows past a handful" as the
/// point where it stops being one.
///
/// RFC 7250 raw public keys (rustls PR #2062) would be strictly cleaner and are the recorded
/// preferred end state — but mosquitto and paho do not implement RFC 7250, so adopting them today
/// would break the integration surface. Whether rustls can negotiate server-X.509 **and** client-RPK
/// simultaneously is ADR 0013 open question 11, unresolved.
///
/// # What it is right now
///
/// A refusal. [`verify_client_cert`](crate::PinnedSpkiClientVerifier::verify_client_cert) and
/// [`verify_tls13_signature`](crate::PinnedSpkiClientVerifier::verify_tls13_signature) return
/// `Err(TlsError::Unimplemented)` unconditionally. **This is the single most dangerous file in the
/// crate to "helpfully" finish**: an `Ok(())` here authorizes every client that connects, looks
/// implemented, and passes any test that only checks the happy path. The only value it is ever
/// correct to return before the SPKI comparison is written is an error.
///
/// TODO(ADR 0013 WP-3): implement over `TrustStore::verify_peer`. The DER `SubjectPublicKeyInfo`
/// extractor it depends on is **WP-1** and lives in `lib-security`, not here — it parses
/// attacker-controlled input, which ADR 0013 calls the sharpest edge in the whole design, and it is
/// specified as bounded, alloc-free, non-recursive, fuzzed and Miri-checked.
pub struct PinnedSpkiClientVerifier<P: SecureProvider> {
    /// The seam holding the pinned anchors. The pin list itself never lives in this crate — it is
    /// Secure-world state reached through `TrustStore`, so Non-Secure code cannot edit the set of
    /// peers it will accept.
    pub(crate) provider: P,
}

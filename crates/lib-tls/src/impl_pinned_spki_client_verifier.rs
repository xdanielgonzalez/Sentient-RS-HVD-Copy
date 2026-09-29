use super::prelude::*;

/// **The most safety-critical stub in the crate.** Both verification methods refuse
/// unconditionally. An `Ok(())` from either of them authorizes every client that connects, while
/// looking implemented and passing any test that checks only the happy path — which is why
/// `tests/fail_closed.rs` asserts the refusal directly rather than trusting review to catch it.
impl<P: SecureProvider> PinnedSpkiClientVerifier<P> {
    /// Bind a verifier to the Secure-world trust store holding the pinned anchors.
    pub fn new(provider: P) -> Self {
        PinnedSpkiClientVerifier { provider }
    }

    /// The seam holding the pin list. Non-Secure code can read the anchors' resolved identity but
    /// cannot edit the set of peers the device will accept — the list lives behind the veneer.
    pub fn provider(&self) -> &P {
        &self.provider
    }

    /// Whether a client certificate is required. **Always `true`, and it is not configurable.**
    ///
    /// ADR 0013 §6: mTLS is mandatory and TLS is the *only* authentication mechanism on the MCU
    /// broker — MQTT CONNECT username/password stays disabled, so there is no second, weaker door.
    /// A client that presents no certificate, or one whose SPKI is not pinned, fails the handshake.
    ///
    /// This returns `true` rather than `Err` for the same reason [`TlsServerStage::is_handshaking`]
    /// returns `true`: the signature cannot carry an error, so it returns the strict answer.
    pub fn client_auth_mandatory(&self) -> bool {
        true
    }

    /// Verify a client's certificate against the pinned SPKI list, resolving the trust anchor that
    /// authorization will be keyed on.
    ///
    /// `now` is `Option` because the device has no real-time clock. Under the `PinnedSpki` model it
    /// is `None` and that is correct — there is no validity window to check. A future `X509Chain`
    /// model **must fail closed on `None`**; the `Option` in the signature is what makes that a
    /// compile-visible obligation rather than a comment somebody deletes.
    ///
    /// TODO(ADR 0013 WP-3): extract the `SubjectPublicKeyInfo` (WP-1's bounded, alloc-free,
    /// non-recursive, fuzzed DER extractor in `lib-security` — **not** a parser written here),
    /// SHA-256 it through `Hasher`, and hand it to `TrustStore::verify_peer`, which fails closed
    /// with `UntrustedChain` when no anchor matches. No chain building, no webpki.
    pub fn verify_client_cert(
        &self,
        _end_entity: &[u8],
        _intermediates: &[&[u8]],
        _now: Option<UnixSeconds>,
    ) -> Result<()> {
        // Fail closed. The only value it is ever correct to return here before the SPKI comparison
        // exists is an error — see this module's doc comment.
        Err(TlsError::Unimplemented)
    }

    /// Verify the client's `CertificateVerify` signature over the handshake transcript — the proof
    /// that the peer actually holds the private half of the pinned key, without which pinning
    /// verifies nothing but a copied certificate.
    ///
    /// TODO(ADR 0013 WP-3): implement over the existing `Verifier` seam. TLS 1.3 only, EC only.
    pub fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &[u8],
        _signature: &[u8],
    ) -> Result<()> {
        // Fail closed.
        Err(TlsError::Unimplemented)
    }
}

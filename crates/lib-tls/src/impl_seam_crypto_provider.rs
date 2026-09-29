use super::prelude::*;

/// Every primitive refuses. Note especially [`SeamCryptoProvider::fill_random`]: a random source
/// that quietly returned zeroes would compromise every key derived from it while appearing to work
/// perfectly, so it errors like the rest.
impl<P: SecureProvider> SeamCryptoProvider<P> {
    /// Bind a provider to the Secure-world seam.
    pub fn new(provider: P) -> Self {
        SeamCryptoProvider { provider }
    }

    /// The seam every primitive routes through.
    pub fn provider(&self) -> &P {
        &self.provider
    }

    /// Fill `out` with cryptographically secure random bytes for the handshake (the server random,
    /// session ids, ECDHE ephemerals).
    ///
    /// TODO(ADR 0013 WP-3): implement `rustls::crypto::SecureRandom` over `Rng`. Note ADR 0013
    /// §Context: the Secure world's RNG is still `InsecureTestRng` seeded from the DWT cycle
    /// counter, so **no honest security claim is possible until WP-5 replaces it** with the
    /// hardware ring-oscillator TRNG — this method being correct is necessary but not sufficient.
    pub fn fill_random(&mut self, _out: &mut [u8]) -> Result<()> {
        Err(TlsError::Unimplemented)
    }

    /// Verify a peer signature under `algorithm` — the `CertificateVerify` leg and, later, any
    /// signed-configuration check.
    ///
    /// TODO(ADR 0013 WP-3): route to `Verifier::verify`. TLS 1.3 only, EC only — no RSA, which is
    /// also what keeps webpki out of the graph.
    pub fn verify_signature(
        &self,
        _public_key: &[u8],
        _message: &[u8],
        _signature: &[u8],
    ) -> Result<()> {
        // Fail closed: an unverified signature is a rejected signature.
        Err(TlsError::Unimplemented)
    }

    /// Resolve the device's own signing identity for the server certificate, as an opaque handle.
    ///
    /// The handle, never bytes: this is the same `KeyPurpose::Identity` key that binds the DDS/TMS
    /// `tms.Identity` announcement (ADR 0013 §4.1) — one trust store, one identity, one custody
    /// tier, reported by one `custody()` accessor.
    ///
    /// TODO(ADR 0013 WP-3): `TrustStore::device_identity(KeyPurpose::Identity)`, wrapped in a rustls
    /// `SigningKey` whose `sign` calls `KeyStore::sign` across the veneer.
    pub fn device_signing_key(&self) -> Result<KeyHandle> {
        Err(TlsError::Unimplemented)
    }
}

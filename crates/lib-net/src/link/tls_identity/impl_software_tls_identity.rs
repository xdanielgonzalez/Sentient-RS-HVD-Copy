use super::prelude::*;

impl SoftwareTlsIdentity {
    /// Build a software-custody identity from a caller-supplied certificate (leaf, DER-encoded)
    /// and its PKCS#8 private key (also DER-encoded). Fails closed if the key DER doesn't parse
    /// as a supported signing key type — this constructor doesn't care where the bytes came from;
    /// this workspace's tests source them from `test_kit::tls::generate_bringup_identity` (a
    /// fresh, self-signed, process-local identity — see `crate::link::tls`'s module docs).
    pub fn from_der(
        cert_der: CertificateDer<'static>,
        key_der: PrivateKeyDer<'static>,
    ) -> Result<Self, TlsError> {
        let key = rustls::crypto::ring::sign::any_supported_type(&key_der)
            .map_err(|_| TlsError::Handshake)?;
        let certified_key = Arc::new(CertifiedKey::new(vec![cert_der], key));
        Ok(SoftwareTlsIdentity { certified_key })
    }
}

impl TlsIdentity for SoftwareTlsIdentity {
    fn custody(&self) -> KeyCustody {
        // Mandatory honesty accessor (see `KeyStore::custody`'s doc): this identity's key lives in
        // normal-world process memory, never in a secure element — report exactly that.
        KeyCustody::SoftwareMemory
    }

    fn certified_key(&self) -> Result<Arc<CertifiedKey>, TlsError> {
        Ok(Arc::clone(&self.certified_key))
    }
}

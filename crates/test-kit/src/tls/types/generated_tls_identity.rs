/// A freshly generated, self-signed TLS identity: DER-encoded certificate + PKCS#8 private key.
/// Raw bytes rather than `rustls` types — this crate stays decoupled from the `rustls`/
/// `lib_net::link::tls` API surface; callers (this crate's own dev consumers) already depend on
/// `rustls` directly and wrap these bytes into `CertificateDer`/`PrivatePkcs8KeyDer` themselves.
#[derive(Debug, Clone)]
pub struct GeneratedTlsIdentity {
    /// The self-signed leaf certificate, DER-encoded.
    pub cert_der: Vec<u8>,
    /// The certificate's private key, PKCS#8 DER-encoded.
    pub key_der: Vec<u8>,
}

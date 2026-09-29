use crate::link::tls::TlsError;
use lib_security::KeyCustody;
use rustls::sign::CertifiedKey;
use std::sync::Arc;

/// Supplies a TLS server with a certificate chain and a signing key, through the
/// `lib-security` custody boundary — see the module docs for why [`crate::link::tls::TlsStage`]
/// depends only on this trait.
pub trait TlsIdentity {
    /// Which custody tier this identity's private key material is actually held under. A
    /// software-backed identity MUST report [`KeyCustody::SoftwareMemory`] — never claim hardware
    /// custody it doesn't have (see [`lib_security::KeyStore::custody`]'s mandatory-honesty contract).
    fn custody(&self) -> KeyCustody;

    /// Build the cert chain + signer this identity presents during the handshake. Fallible: a real
    /// backend (secure-world handle lookup, on-demand PKI fetch) can fail and must not panic.
    fn certified_key(&self) -> Result<Arc<CertifiedKey>, TlsError>;
}

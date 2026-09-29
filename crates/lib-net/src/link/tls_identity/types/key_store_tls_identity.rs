use lib_security::{Algorithm, KeyHandle};
use rustls::pki_types::CertificateDer;
use std::sync::Arc;

/// A [`super::tls_identity::TlsIdentity`] backed by any [`lib_security::KeyStore`]: signing happens *at* the store via an opaque
/// [`KeyHandle`], so a secure-world store's private key never crosses into this crate. This is the
/// structurally distinct path a TrustZone/wolfCrypt-backed `KeyStore` plugs into later — nothing in
/// `TlsStage` or this type changes when the store behind it does.
pub struct KeyStoreTlsIdentity<K> {
    pub(in super::super) store: Arc<K>,
    pub(in super::super) handle: KeyHandle,
    pub(in super::super) algorithm: Algorithm,
    pub(in super::super) cert_chain: Vec<CertificateDer<'static>>,
}

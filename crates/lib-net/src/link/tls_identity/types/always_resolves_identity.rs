use rustls::sign::CertifiedKey;
use std::sync::Arc;

/// A [`rustls::server::ResolvesServerCert`] that always resolves to the same [`CertifiedKey`], ignoring SNI — the
/// same "one identity for every connection" behavior `rustls::ConfigBuilder::with_single_cert`
/// gives, just built from a [`super::tls_identity::TlsIdentity`] instead of raw cert/key bytes.
#[derive(Debug)]
pub(in super::super) struct AlwaysResolvesIdentity(pub(in super::super) Arc<CertifiedKey>);

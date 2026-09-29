use rustls::sign::CertifiedKey;
use std::sync::Arc;

/// A software-custody server identity: a rustls `CertifiedKey` built from caller-supplied
/// cert/key DER via [`SoftwareTlsIdentity::from_der`] (see `crate::link::tls_identity`'s module docs for
/// why the identity is a caller-supplied seam rather than fixed material embedded in this crate).
#[derive(Debug)]
pub struct SoftwareTlsIdentity {
    pub(in super::super) certified_key: Arc<CertifiedKey>,
}

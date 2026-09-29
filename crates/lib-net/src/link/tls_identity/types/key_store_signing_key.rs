use lib_security::{Algorithm, KeyHandle};
use std::sync::Arc;

/// A [`rustls::sign::SigningKey`] that never holds key material itself — every signature is produced by calling
/// back into the [`lib_security::KeyStore`] with the opaque handle.
#[derive(Debug)]
pub(in super::super) struct KeyStoreSigningKey<K> {
    pub(in super::super) store: Arc<K>,
    pub(in super::super) handle: KeyHandle,
    pub(in super::super) algorithm: Algorithm,
}

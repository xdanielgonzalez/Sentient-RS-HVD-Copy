use lib_security::KeyHandle;
use rustls::SignatureScheme;
use std::sync::Arc;

#[derive(Debug)]
pub(in super::super) struct KeyStoreSigner<K> {
    pub(in super::super) store: Arc<K>,
    pub(in super::super) handle: KeyHandle,
    pub(in super::super) scheme: SignatureScheme,
}

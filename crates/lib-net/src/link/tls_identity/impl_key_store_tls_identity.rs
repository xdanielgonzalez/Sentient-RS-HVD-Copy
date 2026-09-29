use super::prelude::*;

impl<K> KeyStoreTlsIdentity<K>
where
    K: KeyStore + Debug + Send + Sync + 'static,
{
    /// `handle` must already reference a key of `algorithm` inside `store` (e.g. from
    /// [`KeyStore::generate`] or [`KeyStore::import_private`]); `cert_chain` is the certificate
    /// chain attesting to that key's public half (leaf first).
    pub fn new(
        store: Arc<K>,
        handle: KeyHandle,
        algorithm: Algorithm,
        cert_chain: Vec<CertificateDer<'static>>,
    ) -> Self {
        KeyStoreTlsIdentity {
            store,
            handle,
            algorithm,
            cert_chain,
        }
    }
}

impl<K> TlsIdentity for KeyStoreTlsIdentity<K>
where
    K: KeyStore + Debug + Send + Sync + 'static,
{
    fn custody(&self) -> KeyCustody {
        self.store.custody()
    }

    fn certified_key(&self) -> Result<Arc<CertifiedKey>, TlsError> {
        let signing_key: Arc<dyn SigningKey> = Arc::new(KeyStoreSigningKey {
            store: Arc::clone(&self.store),
            handle: self.handle,
            algorithm: self.algorithm,
        });
        Ok(Arc::new(CertifiedKey::new(
            self.cert_chain.clone(),
            signing_key,
        )))
    }
}

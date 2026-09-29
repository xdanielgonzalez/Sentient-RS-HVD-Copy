use super::prelude::*;
use super::{scheme_for, sig_algorithm_for};

impl<K> SigningKey for KeyStoreSigningKey<K>
where
    K: KeyStore + Debug + Send + Sync + 'static,
{
    fn choose_scheme(&self, offered: &[SignatureScheme]) -> Option<Box<dyn Signer>> {
        let scheme = scheme_for(self.algorithm)?;
        offered.contains(&scheme).then(|| {
            Box::new(KeyStoreSigner {
                store: Arc::clone(&self.store),
                handle: self.handle,
                scheme,
            }) as Box<dyn Signer>
        })
    }

    fn algorithm(&self) -> SignatureAlgorithm {
        sig_algorithm_for(self.algorithm)
    }
}

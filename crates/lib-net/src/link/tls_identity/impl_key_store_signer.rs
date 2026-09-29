use super::prelude::*;

impl<K> Signer for KeyStoreSigner<K>
where
    K: KeyStore + Debug + Send + Sync + 'static,
{
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, rustls::Error> {
        // Bounded scratch buffer: the largest signature in `Algorithm` (RSA-PKCS1-2048) is 256
        // bytes; 512 leaves headroom without needing an allocation-sized guess.
        let mut out = [0u8; 512];
        let n = self
            .store
            .sign(self.handle, message, &mut out)
            .map_err(|_| rustls::Error::General("KeyStore signing failed".into()))?;
        Ok(out[..n].to_vec())
    }

    fn scheme(&self) -> SignatureScheme {
        self.scheme
    }
}

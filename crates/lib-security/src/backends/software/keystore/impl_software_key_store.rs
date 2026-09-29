use super::prelude::*;

impl<R: Rng> SoftwareKeyStore<R> {
    /// Construct an empty store. `rng` is drawn from ONLY at
    /// [`generate`](KeyStore::generate) time — [`import_private`](KeyStore::import_private) needs
    /// no entropy.
    pub fn new(rng: R) -> Self {
        SoftwareKeyStore {
            rng,
            keys: HVec::new(),
        }
    }

    fn push(&mut self, key: SigningKey) -> Result<KeyHandle> {
        // Arena capacity, not key material, is the failure mode here — `Backend` reports "this
        // backend cannot service the request" without inventing a new `SecurityError` variant.
        let handle = KeyHandle(u32::try_from(self.keys.len()).map_err(|_| SecurityError::Backend)?);
        self.keys.push(key).map_err(|_| SecurityError::Backend)?;
        Ok(handle)
    }

    pub(in crate::backends::software::keystore) fn get(
        &self,
        handle: KeyHandle,
    ) -> Result<&SigningKey> {
        self.keys
            .get(handle.0 as usize)
            .ok_or(SecurityError::KeyNotFound)
    }
}

impl<R: Rng> KeyStore for SoftwareKeyStore<R> {
    fn custody(&self) -> KeyCustody {
        // Mandatory honesty accessor: software keys are never hardware-backed.
        KeyCustody::SoftwareMemory
    }

    fn generate(&mut self, algorithm: Algorithm) -> Result<KeyHandle> {
        if algorithm != Algorithm::EcdsaP256Sha256 {
            return Err(SecurityError::NotImplemented);
        }
        let mut seam = SeamRng(&mut self.rng);
        let key = SigningKey::try_generate_from_rng(&mut seam)?;
        self.push(key)
    }

    fn import_private(&mut self, algorithm: Algorithm, bytes: &[u8]) -> Result<KeyHandle> {
        if algorithm != Algorithm::EcdsaP256Sha256 {
            return Err(SecurityError::NotImplemented);
        }
        let key = SigningKey::from_slice(bytes).map_err(|_| SecurityError::Malformed)?;
        self.push(key)
    }

    fn export_public(&self, handle: KeyHandle, out: &mut [u8]) -> Result<usize> {
        let key = self.get(handle)?;
        let point = key.verifying_key().to_sec1_point(false);
        let bytes = point.as_bytes();
        if out.len() < bytes.len() {
            return Err(SecurityError::BufferTooSmall);
        }
        out[..bytes.len()].copy_from_slice(bytes);
        Ok(bytes.len())
    }

    fn sign(&self, handle: KeyHandle, message: &[u8], out: &mut [u8]) -> Result<usize> {
        let key = self.get(handle)?;
        let signature: Signature = key.try_sign(message).map_err(|_| SecurityError::Backend)?;
        let sig_bytes = signature.to_bytes();
        let sig_bytes: &[u8] = sig_bytes.as_ref();
        if out.len() < sig_bytes.len() {
            return Err(SecurityError::BufferTooSmall);
        }
        out[..sig_bytes.len()].copy_from_slice(sig_bytes);
        Ok(sig_bytes.len())
    }
}

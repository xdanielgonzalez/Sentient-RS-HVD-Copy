use super::prelude::*;
use super::types::WolfKeyStore;

impl Default for WolfKeyStore {
    fn default() -> Self {
        // wolfCrypt software keys are never hardware-backed — report the true (lower) tier.
        WolfKeyStore {
            custody: KeyCustody::SoftwareMemory,
        }
    }
}

impl WolfKeyStore {
    /// Construct a software-custody wolfCrypt key store.
    pub fn new() -> Self {
        WolfKeyStore::default()
    }
}

impl KeyStore for WolfKeyStore {
    fn custody(&self) -> KeyCustody {
        // Mandatory honesty accessor — MUST be SoftwareMemory for software keys.
        self.custody
    }

    fn generate(&mut self, _algorithm: Algorithm) -> Result<KeyHandle> {
        // Fail closed: not yet bound. Requires a handle-keyed key arena + confirmed wolfCrypt
        // keygen types. Do NOT return a fake handle.
        Err(SecurityError::NotImplemented)
    }

    fn import_private(&mut self, _algorithm: Algorithm, _bytes: &[u8]) -> Result<KeyHandle> {
        // Fail closed: not yet bound (see `generate`).
        Err(SecurityError::NotImplemented)
    }

    fn export_public(&self, _handle: KeyHandle, _out: &mut [u8]) -> Result<usize> {
        // No key can exist while generate/import are unbound; refuse rather than fabricate.
        Err(SecurityError::NotImplemented)
    }

    fn sign(&self, _handle: KeyHandle, _message: &[u8], _out: &mut [u8]) -> Result<usize> {
        // Fail closed: signing is unbound until the key arena + wolfCrypt signing-key types are wired.
        Err(SecurityError::NotImplemented)
    }
}

use super::prelude::*;
use super::types::WolfSessionStore;

impl Default for WolfSessionStore {
    fn default() -> Self {
        // wolfCrypt software keys are never hardware-backed — report the true (lower) tier.
        WolfSessionStore {
            custody: KeyCustody::SoftwareMemory,
        }
    }
}

impl WolfSessionStore {
    /// Construct a software-custody wolfCrypt session-cipher store.
    pub fn new() -> Self {
        WolfSessionStore::default()
    }
}

impl SessionCipher for WolfSessionStore {
    fn custody(&self) -> KeyCustody {
        // Mandatory honesty accessor — MUST be SoftwareMemory for software keys.
        self.custody
    }

    fn establish(&mut self) -> Result<KeyHandle> {
        // Fail closed: not yet bound. Requires a handle-keyed session-key arena + confirmed
        // wolfCrypt AES-GCM types.
        Err(SecurityError::NotImplemented)
    }

    fn seal(
        &mut self,
        _handle: KeyHandle,
        _aad: &[u8],
        _buf: &mut [u8],
        _len: usize,
    ) -> Result<usize> {
        // No key can exist while `establish` is unbound; refuse rather than fabricate.
        Err(SecurityError::NotImplemented)
    }

    fn open(
        &mut self,
        _handle: KeyHandle,
        _aad: &[u8],
        _buf: &mut [u8],
        _len: usize,
    ) -> Result<usize> {
        // Fail closed (see `establish`/`seal`).
        Err(SecurityError::NotImplemented)
    }
}

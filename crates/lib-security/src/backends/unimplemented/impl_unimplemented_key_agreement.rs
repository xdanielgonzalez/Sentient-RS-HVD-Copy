use super::prelude::*;

impl KeyAgreement for UnimplementedKeyAgreement {
    /// The weakest tier — see [`UnimplementedTrustStore`]'s custody note.
    fn custody(&self) -> KeyCustody {
        KeyCustody::SoftwareMemory
    }

    fn generate_ephemeral(&mut self, _group: KexGroup) -> Result<KeyHandle> {
        // No handle is ever minted: a fabricated one would flow into `agree` and then into the key
        // schedule, producing traffic keys over a secret that was never computed.
        Err(SecurityError::NotImplemented)
    }

    fn public_share(&self, _handle: KeyHandle, _out: &mut [u8]) -> Result<usize> {
        Err(SecurityError::NotImplemented)
    }

    fn agree(&mut self, _handle: KeyHandle, _peer_share: &[u8]) -> Result<KeyHandle> {
        Err(SecurityError::NotImplemented)
    }
}

use super::prelude::*;

impl TrustStore for UnimplementedTrustStore {
    /// The weakest tier. A placeholder must never report custody it does not have — the honesty
    /// accessor is exactly what an attestation reads.
    fn custody(&self) -> KeyCustody {
        KeyCustody::SoftwareMemory
    }

    /// [`TrustModel::Unconfigured`]: nothing is trusted. Deliberately not `PinnedSpki` — that would
    /// tell a caller "this store tolerates an absent clock", a statement about a working store.
    fn model(&self) -> TrustModel {
        TrustModel::Unconfigured
    }

    /// Zero anchors. A caller may use this to refuse to open the listener at all.
    fn anchor_count(&self) -> usize {
        0
    }

    fn verify_peer(
        &self,
        _credential: &PeerCredential<'_>,
        _now: Option<UnixSeconds>,
        _out: &mut PeerIdentity,
    ) -> Result<()> {
        // Fail closed. `_out` is left untouched: a caller that ignores this `Err` finds whatever it
        // constructed, which grants nothing under default-deny authorization.
        Err(SecurityError::NotImplemented)
    }

    fn device_identity(&self, _purpose: KeyPurpose) -> Result<KeyHandle> {
        // No key is provisioned. Returning a handle here would let a caller "sign" with a key that
        // does not exist and read the resulting failure as a transient backend error.
        Err(SecurityError::NotImplemented)
    }

    fn device_credential(&self, _out: &mut [u8]) -> Result<usize> {
        Err(SecurityError::NotImplemented)
    }
}

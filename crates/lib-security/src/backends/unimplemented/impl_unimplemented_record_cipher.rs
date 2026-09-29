use super::prelude::*;

impl RecordCipher for UnimplementedRecordCipher {
    /// The weakest tier — see [`UnimplementedTrustStore`]'s custody note.
    fn custody(&self) -> KeyCustody {
        KeyCustody::SoftwareMemory
    }

    /// [`RecordProtection::NonSecureExported`] — the weaker of the two claims, reported on the same
    /// principle as [`KeyCustody::SoftwareMemory`] above. This stub does no record crypto anywhere,
    /// so claiming [`RecordProtection::SecureWorld`] would assert a Secure-world boundary that does
    /// not exist. The consequence is intended: a `fips` build, which forbids
    /// [`RecordProtection::NonSecureExported`], refuses this backend outright.
    fn protection(&self) -> RecordProtection {
        RecordProtection::NonSecureExported
    }

    fn seal_record(
        &mut self,
        _key: KeyHandle,
        _seq: u64,
        _aad: &[u8],
        _buf: &mut [u8],
        _len: usize,
    ) -> Result<usize> {
        // `_buf` is left exactly as the caller supplied it — still plaintext. Returning `Ok` here
        // would put that plaintext on a socket the caller believes is encrypted.
        Err(SecurityError::NotImplemented)
    }

    fn open_record(
        &mut self,
        _key: KeyHandle,
        _seq: u64,
        _aad: &[u8],
        _buf: &mut [u8],
        _len: usize,
    ) -> Result<usize> {
        // Returning `Ok` here would hand unauthenticated attacker bytes to the broker as verified
        // plaintext.
        Err(SecurityError::NotImplemented)
    }
}

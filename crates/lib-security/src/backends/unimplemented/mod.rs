//! Fail-closed placeholder backends for the ADR 0013 transport-security seam.
//!
//! **Every operation in this module returns
//! [`SecurityError::NotImplemented`](crate::SecurityError::NotImplemented). Nothing here verifies,
//! agrees, derives, encrypts, decrypts or tells the time.** These types exist so that
//! [`TrustStore`](crate::TrustStore), [`KeyAgreement`](crate::KeyAgreement),
//! [`KeyDerivation`](crate::KeyDerivation), [`RecordCipher`](crate::RecordCipher) and
//! [`TrustedTime`](crate::TrustedTime) have a compiling inhabitant — the seam can be wired end to
//! end, `lib-tls` can be written against it, and the veneer ABI can be shaped — before ADR 0013
//! WP-2 supplies real implementations.
//!
//! ## Why they are named `Unimplemented*` rather than WP-2's `Software*`/`StubTime`
//!
//! ADR 0013 §7 WP-2 names the real host backends `SoftwareTrustStore`, `SoftwareKeyAgreement`,
//! `SoftwareKeyDerivation`, `SoftwareRecordCipher` and `StubTime`. Those names are reserved for
//! implementations that actually work. A placeholder carrying a working implementation's name is
//! the exact hazard this module is trying to avoid: a reader — or a claim, or an attestation —
//! seeing `SoftwareTrustStore` in the wiring concludes that peer verification happens. The names
//! here cannot be misread, and WP-2 replaces the types rather than quietly changing their behavior.
//!
//! ## The one rule
//!
//! **No stub may return success.** A `verify_peer` that returned `Ok(())` would authorize every
//! peer; an `open_record` that returned `Ok` would hand attacker bytes to the broker as verified
//! plaintext; a `generate_ephemeral` that returned a fabricated
//! [`KeyHandle`](crate::KeyHandle) would produce a key schedule over a secret that was never
//! computed. Every method here is a refusal, and the `unimplemented_stubs_never_succeed` test in
//! `tests/transport_seam.rs` is what keeps it that way.
//!
//! The infallible accessors that *must* return a value report the weakest honest answer:
//! [`KeyCustody::SoftwareMemory`](crate::KeyCustody::SoftwareMemory),
//! [`TrustModel::Unconfigured`](crate::TrustModel::Unconfigured),
//! [`RecordProtection::NonSecureExported`](crate::RecordProtection::NonSecureExported),
//! [`TimeTrust::Unavailable`](crate::TimeTrust::Unavailable) — never a posture stronger than the
//! (nonexistent) implementation behind them. `UnimplementedTime::uptime_millis` is the single
//! method that cannot refuse, because the trait makes it infallible; it is frozen at zero and its
//! liveness consequence is documented on [`UnimplementedTime`].
//!
//! This module has no feature gate and no dependencies — it is pure refusal, and it must be
//! available in every build configuration, including one with no concrete backend selected.
//!
//! See ADR `docs/decisions/0013-transport-security-trust-store.md` §1 and §7.

mod prelude;
mod types;

mod impl_unimplemented_key_agreement;
mod impl_unimplemented_key_derivation;
mod impl_unimplemented_record_cipher;
mod impl_unimplemented_time;
mod impl_unimplemented_trust_store;

pub use types::*;

#[cfg(test)]
use prelude::*;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// The honesty accessors report the weakest posture, never a stronger one. An attestation
    /// reading these must not be able to conclude that anything is hardware-backed, configured,
    /// Secure-world-protected, or clocked.
    #[test]
    fn accessors_report_the_weakest_honest_posture() {
        assert_eq!(
            UnimplementedTrustStore.custody(),
            KeyCustody::SoftwareMemory
        );
        assert_eq!(UnimplementedTrustStore.model(), TrustModel::Unconfigured);
        assert_eq!(UnimplementedTrustStore.anchor_count(), 0);
        assert_eq!(
            UnimplementedKeyAgreement.custody(),
            KeyCustody::SoftwareMemory
        );
        assert_eq!(
            UnimplementedRecordCipher.custody(),
            KeyCustody::SoftwareMemory
        );
        assert_eq!(
            UnimplementedRecordCipher.protection(),
            RecordProtection::NonSecureExported,
            "a stub does no record crypto anywhere and must not claim a Secure-world boundary"
        );
        assert_eq!(UnimplementedTime.trust(), TimeTrust::Unavailable);
    }

    /// A refused `seal_record` must leave the caller's buffer untouched — the plaintext is still
    /// plaintext, which is the state a caller that checked the `Err` expects.
    #[test]
    fn refused_seal_does_not_touch_the_buffer() {
        let mut cipher = UnimplementedRecordCipher;
        let mut buf = *b"plaintext-record--------";
        let before = buf;
        assert!(cipher
            .seal_record(KeyHandle(0), 0, b"aad", &mut buf, 9)
            .is_err());
        assert_eq!(buf, before);
    }

    /// A refused `expand_to_bytes` must not manufacture an all-zero "derived key".
    #[test]
    fn refused_derivation_does_not_write_output() {
        let mut kdf = UnimplementedKeyDerivation;
        let mut out = [0xAAu8; 32];
        assert!(kdf
            .expand_to_bytes(KeyHandle(0), b"key", b"", &mut out)
            .is_err());
        assert_eq!(out, [0xAAu8; 32]);
    }
}

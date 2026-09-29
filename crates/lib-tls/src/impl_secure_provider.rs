use super::prelude::*;

/// The blanket impl that makes [`SecureProvider`] an aggregation rather than a god-trait: any type
/// satisfying the eight seam traits is a `SecureProvider` automatically, with nothing extra to
/// write and nothing to keep in sync.
///
/// The bound list is repeated verbatim from the trait declaration — that repetition is what a
/// blanket impl costs, and it is cheaper than the alternative (a `CryptoOps` trait re-declaring
/// sign / verify / random / digest, giving every backend author two places to get one operation
/// wrong).
impl<T> SecureProvider for T where
    T: TrustStore
        + KeyStore
        + KeyAgreement
        + KeyDerivation
        + RecordCipher
        + Verifier
        + Hasher
        + Rng
{
}

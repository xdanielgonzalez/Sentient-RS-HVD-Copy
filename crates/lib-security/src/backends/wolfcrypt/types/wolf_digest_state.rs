use super::wolf::wolf;

/// The concrete wolfCrypt SHA-2 hasher for the selected [`crate::Digest`].
pub(in crate::backends::wolfcrypt) enum WolfDigestState {
    Sha256(wolf::Sha256),
    Sha384(wolf::Sha384),
    Sha512(wolf::Sha512),
}

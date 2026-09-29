use super::wolf_digest_state::WolfDigestState;
use crate::traits::Digest;

/// [`crate::Hasher`] backed by wolfCrypt SHA-2, delegating through `digest::Digest`.
pub struct WolfHasher {
    pub(in crate::backends::wolfcrypt) which: Digest,
    pub(in crate::backends::wolfcrypt) state: WolfDigestState,
}

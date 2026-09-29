//! Shared import surface for the wolfcrypt module.

pub(crate) use crate::error::{Result, SecurityError};
pub(crate) use crate::profile::KeyCustody;
pub(crate) use crate::traits::{
    Algorithm, Digest, Hasher, KeyHandle, KeyStore, Rng, SessionCipher, Verifier,
};

// RustCrypto trait imports — these are the STABLE surfaces we bind to. wolfCrypt's concrete types
// (in `wolf` below) implement these same traits, so the delegation typechecks once the paths are
// confirmed. Trait-crate versions are pinned in Cargo.toml to match wolfssl-wolfcrypt 2.0.
pub(crate) use digest::{Digest as _RcDigest, FixedOutputReset as _RcFixedOutputReset};
pub(crate) use signature::Verifier as _RcVerifier;

pub(crate) use super::types::wolf;
pub(crate) use super::types::WolfDigestState;

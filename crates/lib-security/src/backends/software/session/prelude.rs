//! Shared import surface for the `session` module's `impl_*` files ONLY — see the crate's Code
//! Organization doctrine: type files import directly, never this prelude.

pub use heapless::Vec as HVec;

pub use aes_gcm::{AeadInPlace, Aes256Gcm, Key, KeyInit, Nonce, Tag};

pub use zeroize::Zeroize;

pub use crate::error::{Result, SecurityError};
pub use crate::profile::KeyCustody;
pub use crate::traits::{KeyHandle, Rng, SessionCipher};

pub use super::types::*;

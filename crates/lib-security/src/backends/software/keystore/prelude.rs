//! Private prelude: re-exports the module's types plus the original external `use` lines,
//! with relative paths bumped one level for the new nesting depth.

pub use super::types::*;

pub use heapless::Vec as HVec;
pub use p256::ecdsa::signature::Signer;
pub use p256::ecdsa::{Signature, SigningKey};
pub use p256::elliptic_curve::Generate;

pub use crate::error::{Result, SecurityError};
pub use crate::profile::KeyCustody;
pub use crate::traits::{Algorithm, KeyHandle, KeyStore, Rng};

pub use super::super::rng::SeamRng;

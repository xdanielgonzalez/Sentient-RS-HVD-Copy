//! Shared import surface for the rng module.

pub use p256::elliptic_curve::rand_core::{TryCryptoRng as RcTryCryptoRng, TryRng as RcTryRng};

pub use crate::error::{Result, SecurityError};
pub use crate::traits::Rng;

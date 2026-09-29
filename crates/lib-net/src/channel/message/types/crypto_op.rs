#[cfg(feature = "packed")]
use serde::{Deserialize, Serialize};

/// The operation requested at the crypto boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "packed", derive(Serialize, Deserialize))]
pub enum CryptoOp {
    /// Encrypt/authenticate the plaintext into a sealed record.
    Seal,
    /// Decrypt/verify a sealed record back to plaintext.
    Open,
}

//! Shared import surface for the `unimplemented` module's `impl_*` files ONLY — see the crate's
//! Code Organization doctrine: type files import directly, never this prelude.

pub use crate::error::{Result, SecurityError};
pub use crate::profile::{KeyCustody, KeyPurpose};
pub use crate::traits::{
    KexGroup, KeyAgreement, KeyDerivation, KeyHandle, PeerCredential, PeerIdentity, RecordCipher,
    RecordProtection, TimeTrust, TrustModel, TrustStore, TrustedTime, UnixSeconds,
};

pub use super::types::*;

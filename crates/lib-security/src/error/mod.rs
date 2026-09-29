//! The typed, **fail-closed** error surface for all security operations.
//!
//! Every fallible security operation returns `Result<_, SecurityError>`. There is no panic path and
//! no `unwrap` in logic. The cardinal rule: an operation that cannot *prove* success returns an
//! error — a verifier never returns "ok" by default, an unimplemented backend returns
//! [`SecurityError::NotImplemented`], not a success. Callers must treat any `Err` as "deny".

mod prelude;
mod types;

pub use types::*;

mod impl_display;
mod impl_error;

/// Result alias for security operations.
pub type Result<T> = core::result::Result<T, SecurityError>;

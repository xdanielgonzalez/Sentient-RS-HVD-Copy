//! Type definitions only — one primary type per file, no `impl` blocks. Behavior lives in the
//! `impl_*.rs` files at the crate root.
//!
//! Per the repo's file doctrine these files import **directly** (`use lib_security::TrustStore;`,
//! `use super::secure_provider::SecureProvider;`) and never through the crate's `prelude`, which
//! re-exports this module and would form a `types` ⇄ `prelude` glob cycle. Only the `impl_*.rs`
//! files use the prelude.

mod pinned_spki_client_verifier;
mod seam_crypto_provider;
mod seam_time_provider;
mod secure_provider;
mod tls_error;
mod tls_result;
mod tls_server_stage;

pub use pinned_spki_client_verifier::*;
pub use seam_crypto_provider::*;
pub use seam_time_provider::*;
pub use secure_provider::*;
pub use tls_error::*;
pub use tls_result::*;
pub use tls_server_stage::*;

//! Shared imports for the `cdr` module's `impl_*.rs` files. Per CLAUDE.md, ONLY the `impl_*.rs`
//! files reach up one level to this prelude (acyclic — impls are not members of `types/`); the type
//! files under `types/` import their siblings/externals directly.

pub use super::types::*;

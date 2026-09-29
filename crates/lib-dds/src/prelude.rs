//! Private prelude for the crate-root `impl_*.rs` files ONLY (per the repo's import discipline). Type
//! files under `types/` import directly and must NOT use this. Re-exports the seam types + the
//! externals the root impls share.

pub use super::types::*;

pub use tokio::sync::mpsc;

//! Private glob-import surface: the crate's types plus the externals `impl_*.rs` files (and
//! `mod.rs` itself) pull in via `use crate::mesh::prelude::*;` / `use super::prelude::*;`.

pub use super::types::*;

// `CanonicalReading` is spelled out by name only in `impl_condense`'s `#[cfg(test)]` fixtures
// (production code reaches its fields through `CondenseInput::reading` without ever naming the
// type) — a plain `cargo check` (no `--tests`) therefore sees this re-export as unused.
#[allow(unused_imports)]
pub use lib_parse::canonical::{CanonicalFault, CanonicalReading};

// Only available when this crate's `locate` feature is on (std/tokio-only) — the MCU's bare
// `mesh` build never sees `crate::locate` at all (it's `#[cfg(feature = "locate")]` in lib.rs).
// Spelled out by name only in `impl_condense`'s `#[cfg(test)]` `with_location` fixtures
// (production code reaches it through `CondenseInput::loc` without ever naming the type), so a
// plain `cargo check` (no `--tests`) sees this re-export as unused.
#[cfg(feature = "locate")]
#[allow(unused_imports)]
pub use crate::locate::LocationFix;

//! Typed error surface for Seplos protocol decode. No panics, no `unwrap` in logic — every
//! fault path is a variant here. `#[non_exhaustive]` so new fault cases do not break callers.

mod prelude;
mod types;

pub use types::*;

mod impl_seplos_error;

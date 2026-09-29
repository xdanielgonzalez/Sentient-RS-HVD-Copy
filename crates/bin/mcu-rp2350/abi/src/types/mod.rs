//! Type definitions for the frozen `sv_*` ABI. One primary type per file (CLAUDE.md code
//! organization); implementations live in the sibling `impl_*.rs` files at the crate root, not here.

mod sv_custody;
mod sv_error;

pub use sv_custody::SvCustody;
pub use sv_error::SvError;

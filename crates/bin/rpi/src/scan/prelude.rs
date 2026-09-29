//! Private prelude: re-exports the module's types plus the externals `impl_*.rs` files share.

pub use super::types::*;

pub use crate::config::Config;
pub use lib_io::host::SerialKind;
pub use lib_io::transport::SerialParams;

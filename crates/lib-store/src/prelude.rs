//! Private prelude: re-exports the module's own types plus the external `use` lines the
//! type/impl files need

pub use super::types::*;

pub use heapless::HistoryBuf;
pub use lib_canonical::{CanonicalFault, CanonicalReading};

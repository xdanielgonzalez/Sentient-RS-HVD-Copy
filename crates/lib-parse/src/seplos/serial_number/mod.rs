//! Seplos `0xA4` serial-number decode.
//!
//! INFO bytes are interpreted as **strict** ASCII (no lossy `errors="ignore"` as the prototype
//! did), trimmed of surrounding whitespace, then a single leading `<` is stripped if present.
//!
//! CONTRACT ONLY — body is a pipeline FILL target.

mod prelude;
mod types;

pub use types::*;

mod impl_serial_number;

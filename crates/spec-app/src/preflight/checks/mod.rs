//! Generic, platform-agnostic checks. Each is a concrete unit struct implementing
//! [`crate::preflight::Check`] against whichever [`crate::preflight::capabilities`] trait its
//! context needs to satisfy.

mod config_valid;
mod serial_present;

pub use config_valid::ConfigValid;
pub use serial_present::SerialPresent;

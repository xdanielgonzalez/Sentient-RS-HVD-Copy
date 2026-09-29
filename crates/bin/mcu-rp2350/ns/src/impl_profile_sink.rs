//! `lib_profile::ProfileSink` impl over defmt/RTT.
//!
//! Used by `lib_profile::report()` to emit profiler output when the `profile` feature is enabled.

use lib_profile::ProfileSink;

/// Zero-sized `ProfileSink` impl — logs via defmt (RTT in the MCU build).
pub(crate) struct DefmtProfileSink;

impl ProfileSink for DefmtProfileSink {
    fn log_line(&self, line: &str) {
        defmt::println!("[profile] {}", line);
    }
}

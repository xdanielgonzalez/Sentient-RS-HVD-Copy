use crate::preflight::{Outcome, MAX_CHECKS};

/// The bounded, ordered results of a preflight run.
///
/// Warnings never block ([`ok`](Report::ok) only counts failures), and
/// [`ok_except`](Report::ok_except) is the narrow degraded-start gate: it excuses exactly one
/// named check while every other failure still blocks.
///
/// ```rust
/// use spec_app::preflight::{Outcome, Report};
///
/// let mut report = Report::new();
/// report.record(("serial_present", Outcome::Fail("configured serial device not present")));
/// report.record(("config_valid", Outcome::Pass));
/// report.record(("clock_sync", Outcome::Warn("NTP not yet converged")));
///
/// assert_eq!(report.passed(), 1);
/// assert_eq!(report.warned(), 1);
/// assert_eq!(report.failed(), 1);
/// assert!(!report.ok());
///
/// // Degraded-start: a host platform may proceed without its serial port (bug #172)...
/// assert!(report.ok_except("serial_present"));
/// // ...but excusing a different check does not forgive the serial failure.
/// assert!(!report.ok_except("config_valid"));
/// ```
#[derive(Debug, Clone)]
pub struct Report {
    pub(crate) entries: heapless::Vec<(&'static str, Outcome), MAX_CHECKS>,
}

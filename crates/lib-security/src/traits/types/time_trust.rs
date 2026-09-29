/// How much a clock's wall-clock reading may be believed. The honesty accessor of
/// [`TrustedTime`](super::trusted_time::TrustedTime), and the reason the clock is its own seam.
///
/// **Only [`TimeTrust::Authenticated`] may feed a security decision.** The rule exists because an
/// attacker who controls unauthenticated time can resurrect expired or revoked credentials — so
/// unauthenticated time is an availability input (log timestamps, display) and never an input to a
/// validity or revocation check. ADR 0013 §5 states this about SNTP by name.
///
/// See ADR `docs/decisions/0013-transport-security-trust-store.md` §5.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TimeTrust {
    /// No wall-clock reading is available at all. [`now`](super::trusted_time::TrustedTime::now)
    /// fails closed. This is the state the device is in with no RTC and no time sync.
    Unavailable,
    /// A wall-clock reading exists but its source is unauthenticated (e.g. plain SNTP). Usable for
    /// log timestamps and human display **only**. Passing it to a validity or revocation check is
    /// a security defect, not a degradation.
    Unauthenticated,
    /// The reading comes from a cryptographically authenticated source (e.g. Roughtime, or
    /// enclave-signed time). This is the only variant a validity window, a revocation check, or any
    /// other security decision may consume.
    Authenticated,
}

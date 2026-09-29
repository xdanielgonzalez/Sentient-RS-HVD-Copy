use super::time_trust::TimeTrust;
use super::unix_seconds::UnixSeconds;
use crate::error::Result;

/// **The clock as its own seam, with its own honesty accessor.**
///
/// Time is a security input — validity windows, revocation freshness, replay windows all consume
/// it — so it gets the same treatment as key custody: the value and the *trustworthiness of the
/// value* are reported separately, and only the trustworthy variant may drive a decision. An
/// attacker who controls unauthenticated time can resurrect expired or revoked credentials, which
/// is why "we have a clock" is not the same statement as "we can check expiry".
///
/// # What a real implementation MUST do
///
/// * **Report [`trust`](TrustedTime::trust) truthfully.** A clock fed by plain SNTP is
///   [`TimeTrust::Unauthenticated`] — never [`TimeTrust::Authenticated`], however accurate it is.
///   [`TimeTrust::Authenticated`] means the reading is cryptographically bound to a source we
///   trust (Roughtime, enclave-signed time), and it is the **only** variant a validity or
///   revocation check may consume.
/// * **Fail closed with no clock.** [`now`](TrustedTime::now) MUST return an error, not an epoch
///   or a fabricated value, when [`trust`](TrustedTime::trust) is [`TimeTrust::Unavailable`].
///   Callers that need a security-usable instant pass `None` to
///   [`TrustStore::verify_peer`](super::trust_store::TrustStore::verify_peer), which then fails
///   closed for any model that needs a validity window.
/// * **Keep [`uptime_millis`](TrustedTime::uptime_millis) monotonic and independent.** It is a
///   boot-relative counter for timeouts and rate limits, is always available, and never goes
///   backwards. It is deliberately not a `Result` and deliberately not a wall clock: no security
///   decision may be derived from it, and no wall-clock outage may stop a timeout from firing.
/// * **Never let the two mix.** A wall-clock reading must not be synthesized from uptime, and
///   uptime must not be adjusted by a time sync.
///
/// See ADR `docs/decisions/0013-transport-security-trust-store.md` §5.
pub trait TrustedTime {
    /// How much [`now`](TrustedTime::now) may be believed. Security decisions may only consume
    /// [`TimeTrust::Authenticated`]; [`TimeTrust::Unauthenticated`] is an availability input (log
    /// timestamps) and is **never** an input to a validity or revocation check.
    fn trust(&self) -> TimeTrust;

    /// The current wall-clock instant. Fails closed with [`crate::SecurityError::NotImplemented`]
    /// or [`crate::SecurityError::Backend`] when no reading is available — never a fabricated
    /// epoch. Check [`trust`](TrustedTime::trust) before using the value for anything but a log.
    fn now(&self) -> Result<UnixSeconds>;

    /// Always available: monotonic milliseconds since boot, for timeouts and rate limits. Never a
    /// security input.
    fn uptime_millis(&self) -> u64;
}

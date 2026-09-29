/// A [`TrustedTime`](crate::TrustedTime) with no clock at all.
///
/// [`trust`](crate::TrustedTime::trust) reports [`TimeTrust::Unavailable`](crate::TimeTrust) and
/// [`now`](crate::TrustedTime::now) fails closed with
/// [`SecurityError::NotImplemented`](crate::SecurityError::NotImplemented) — it never returns a
/// fabricated epoch, because a caller comparing a certificate's `notAfter` against a fake instant
/// would either accept every expired credential or reject every valid one, and both are silent.
///
/// **`uptime_millis` is frozen at zero, and that has a liveness consequence**: a caller using it
/// for timeouts sees no time pass, so timeouts never fire and rate-limit windows never reset. That
/// direction is deny-biased for rate limits and hang-biased for timeouts — an availability defect,
/// deliberately visible rather than papered over with a plausible-looking counter. The real MCU
/// implementation is an Embassy monotonic tick.
///
/// ADR 0013 WP-2 names this slot `StubTime`; the name here is explicit about the fact that nothing
/// works, so that a build wiring it cannot read as configured.
#[derive(Debug, Clone, Copy, Default)]
pub struct UnimplementedTime;

use super::prelude::*;

impl<T: TrustedTime> SeamTimeProvider<T> {
    /// Bind a time provider to a clock seam.
    pub fn new(clock: T) -> Self {
        SeamTimeProvider { clock }
    }

    /// The underlying clock, which reports its own trust level. This type never upgrades it: an
    /// `Unauthenticated` clock stays `Unauthenticated` all the way to any caller that asks.
    pub fn clock(&self) -> &T {
        &self.clock
    }

    /// Wall-clock seconds for rustls.
    ///
    /// Refuses in the skeleton rather than returning the epoch. ADR 0013 §5 does specify
    /// epoch-on-`Unavailable` as the implemented behavior — but a stub that already returns a
    /// plausible timestamp is indistinguishable from a working clock, so the skeleton fails instead
    /// of guessing.
    ///
    /// TODO(ADR 0013 WP-3): implement `rustls::time_provider::TimeProvider` over
    /// `TrustedTime::now`. The rule that must survive the implementation: only
    /// `TimeTrust::Authenticated` may feed a validity or revocation check, and this device has no
    /// such source — so while the pinning model holds, nothing consumes this for a security
    /// decision at all.
    pub fn now(&self) -> Result<UnixSeconds> {
        Err(TlsError::Unimplemented)
    }

    /// Monotonic milliseconds since boot — always available, and the *only* time value that is
    /// legitimate to use here today, for timeouts and rate limits.
    ///
    /// TODO(ADR 0013 WP-3): delegate to `TrustedTime::uptime_millis`. It refuses for now so the
    /// skeleton has exactly one behavior and no caller can start depending on a half-wired one.
    pub fn uptime_millis(&self) -> Result<u64> {
        Err(TlsError::Unimplemented)
    }
}

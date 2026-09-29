use super::prelude::*;

impl TrustedTime for UnimplementedTime {
    /// No clock at all. This is what makes callers pass `now: None` to
    /// [`TrustStore::verify_peer`], which any validity-window trust model must refuse.
    fn trust(&self) -> TimeTrust {
        TimeTrust::Unavailable
    }

    fn now(&self) -> Result<UnixSeconds> {
        // Never a fabricated epoch: a comparison against a fake instant either accepts every
        // expired credential or rejects every valid one, and does so silently.
        Err(SecurityError::NotImplemented)
    }

    /// Frozen at zero. The trait requires an infallible value, so this is the one method here that
    /// cannot refuse — see [`UnimplementedTime`]'s doc for the liveness consequence (timeouts never
    /// fire, rate-limit windows never reset). Uptime is never a security input, so a frozen value
    /// cannot authorize anything.
    fn uptime_millis(&self) -> u64 {
        0
    }
}

use lib_security::TrustedTime;

/// The rustls `TimeProvider`, backed by the `TrustedTime` seam — a skeleton (ADR 0013 §5, WP-3).
///
/// # Why a device with no clock still needs one
///
/// rustls requires a `TimeProvider`, and the `no_std` default is unavailable. This device has no
/// real-time clock. Those two facts meet here.
///
/// The resolution is that **nothing in our design consumes this value for a security decision**.
/// Client authentication is pinned SPKI, which has no validity window (see
/// [`PinnedSpkiClientVerifier`](crate::PinnedSpkiClientVerifier)); our own server certificate's
/// validity is enforced by *clients* against *their* clocks, which is their trust decision and not
/// ours. So a shallow clock is safe **only while the pinning model holds** — and that coupling is
/// the whole reason this type exists as a named seam rather than as an inline closure. A later
/// switch to chain validation makes the clock load-bearing, and it must not silently inherit a stub.
///
/// Note that [`TrustedTime`] is deliberately **not** in the [`SecureProvider`](crate::SecureProvider)
/// bound list. Time is a separate seam with its own honesty accessor, precisely so that a caller
/// cannot obtain a timestamp incidentally while reaching for crypto.
///
/// # The rule that must not be softened
///
/// **Unauthenticated time is not a security control.** An attacker who controls unauthenticated
/// time can resurrect expired or revoked credentials. SNTP may be wired for log timestamps and
/// reported as `TimeTrust::Unauthenticated`; it may **never** feed a validity or revocation check.
/// Only `TimeTrust::Authenticated` — Roughtime or enclave-signed time, neither of which this device
/// has — may do that.
///
/// # What it is right now
///
/// A refusal. [`now`](crate::SeamTimeProvider::now) returns `Err(TlsError::Unimplemented)` rather
/// than the Unix epoch. ADR 0013 §5 does specify epoch-on-`Unavailable` as the *implemented*
/// behavior, and that will be correct once WP-3 wires it — but a stub that already returns a
/// plausible timestamp cannot be distinguished from a working clock by any caller or test, so the
/// skeleton refuses instead.
///
/// TODO(ADR 0013 WP-3): implement `rustls::time_provider::TimeProvider` over `TrustedTime::now`,
/// returning the epoch when `trust()` is `Unavailable`, and assert in a test that no validity check
/// consumes it while `model()` is `PinnedSpki`.
pub struct SeamTimeProvider<T: TrustedTime> {
    /// The clock seam. Reports its own trust level; this type never upgrades it.
    pub(crate) clock: T,
}

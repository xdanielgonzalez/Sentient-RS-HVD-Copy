/// Test-only stand-in for a bound `lib-security` [`lib_security::TokenVerifier`] backend.
///
/// A concrete software `TokenVerifier` now ships in `lib_security::backends::software`
/// (default features; see `SoftwareTokenVerifier`) — the wolfCrypt backend
/// (`lib-security/src/backends/wolfcrypt.rs`) still implements none of it. This fake remains
/// as a lightweight test double: it exercises the broker's seam-routed wiring end-to-end (accept
/// on a correct, unexpired, correctly-scoped token; fail closed on a wrong secret, wrong
/// issuer/audience, or expiry) without pulling in real cryptography or asserting anything about
/// cryptographic strength — [`lib_mqtt::broker::seam_auth`] takes any `TokenVerifier`, so swapping this fake for
/// `SoftwareTokenVerifier` (or a wolfCrypt-backed one, once bound) at call sites requires no
/// change to `lib-mqtt`.
///
/// Token wire format: `subject|issuer|audience|expires_at|shared_secret` — a shared-secret compare
/// against `trusted_key`, not a real signature, kept intentionally simple since this is a test
/// double, not a security boundary.
pub struct FakeTokenVerifier;

//! `lib-mqtt` broker test helpers + `FakeTokenVerifier`.
//!
//! Formerly `lib-mqtt/tests/support/mod.rs` (retired in #99); merged here so all test-support
//! lives in one crate.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod prelude;
mod types;

pub use types::*;

mod impl_fake_token_verifier;

use prelude::*;

/// Reserve a free ephemeral TCP port: bind a throwaway listener on `127.0.0.1:0`, read back the
/// OS-assigned port from `local_addr()`, then drop the listener immediately so the real broker can
/// bind that same concrete port right after.
///
/// This — not a fixed port list, and not asking rumqttd to bind port `0` directly (observed
/// unreliable: rumqttd's listener setup does not reliably surface *which* concrete port it ended
/// up bound to) — is what actually kills the concurrent-CI-collision class: multiple CI runs (or
/// local + CI) on the same host no longer fight over the same small set of fixed ports.
pub fn reserve_ephemeral_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .expect("bind an ephemeral port")
        .local_addr()
        .expect("read back the OS-assigned local address")
        .port()
}

/// Start the embedded broker on a freshly reserved ephemeral port. Retries a handful of times in
/// the (rare) case another process wins the race for the same port between the scavenge and the
/// broker's own bind — never a fixed pool.
pub fn start_broker() -> (LocalBroker, u16) {
    for _ in 0..8 {
        let port = reserve_ephemeral_port();
        let cfg = BrokerConfig {
            id: "sentient".into(),
            host: "127.0.0.1".into(),
            port,
            auth: None,
        };
        if let Ok(b) = LocalBroker::start(cfg) {
            return (b, port);
        }
    }
    panic!("could not bind a test broker on any reserved ephemeral port after 8 attempts");
}

// =================================================================================================
// Broker-side CONNECT auth (routes through the `lib-security` seam) — test doubles + helpers.
// =================================================================================================

/// Shared secret backing [`FakeTokenVerifier`] in tests — the `trusted_key` a real deployment would
/// source from a bound `KeyStore`/token backend. Test-only; never a production credential.
pub const AUTH_TRUSTED_KEY: &[u8] = b"test-only-trusted-key-do-not-ship";
pub const AUTH_ISSUER: &str = "sentient-test-issuer";
pub const AUTH_AUDIENCE: &str = "lib-mqtt-broker";

/// A well-formed token for `subject` that verifies under [`FakeTokenVerifier`] (correct secret,
/// correct issuer/audience, far-future expiry).
pub fn valid_token(subject: &str) -> String {
    format!(
        "{subject}|{AUTH_ISSUER}|{AUTH_AUDIENCE}|9999999999|{}",
        String::from_utf8_lossy(AUTH_TRUSTED_KEY)
    )
}

/// A token for `subject` with the wrong shared secret — [`FakeTokenVerifier`] denies it
/// (`SecurityError::VerificationFailed`), simulating a bad/forged credential.
pub fn invalid_token(subject: &str) -> String {
    format!("{subject}|{AUTH_ISSUER}|{AUTH_AUDIENCE}|9999999999|wrong-secret")
}

/// Start the embedded broker, on a freshly reserved ephemeral port, with CONNECT authorization
/// enabled via [`seam_auth`] over the [`FakeTokenVerifier`] test double (see its doc comment — a
/// real `SoftwareTokenVerifier` ships in `lib-security`, but this fake keeps the test
/// lightweight and dependency-free).
pub fn start_authenticated_broker() -> (LocalBroker, u16) {
    for _ in 0..8 {
        let port = reserve_ephemeral_port();
        let cfg = BrokerConfig {
            id: "sentient".into(),
            host: "127.0.0.1".into(),
            port,
            auth: Some(seam_auth(
                Arc::new(FakeTokenVerifier),
                AUTH_TRUSTED_KEY.to_vec(),
                AUTH_ISSUER.into(),
                AUTH_AUDIENCE.into(),
            )),
        };
        if let Ok(b) = LocalBroker::start(cfg) {
            return (b, port);
        }
    }
    panic!("could not bind an authenticated test broker on any reserved ephemeral port after 8 attempts");
}

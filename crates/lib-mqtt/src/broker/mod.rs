//! Embedded rumqttd MQTT server + a [`Publisher`] over its in-process Link.
//!
//! `LocalBroker::start` boots an rumqttd broker with a TCP listener (`host:port`) that local
//! subscribers and the external **mosquitto** relay connect to, plus an in-process **Link** used to
//! publish mapped TMS payloads without a loopback TCP client. `BrokerPublisher` wraps the `LinkTx`.
//!
//! **SPIKE (gap-closure scaffolding, #237):** `BrokerSubscriber` (the `LinkRx` counterpart to
//! `BrokerPublisher`) is a type-only stub — `LocalBroker::start` (`impl_local_broker.rs:78`)
//! currently drops its own `_link_rx` entirely, never subscribing or draining it. Whether
//! `LinkRx` actually delivers publishes in-process once subscribed (and whether `LinkTx::publish`
//! exposes any retain capability) is probed by `tests/link_rx_spike.rs` — see its doc comment for
//! the answer this session recorded. The TMS Request/Reply design (`bin-rpi/src/tasks/
//! reply_listener.rs`) depends on the answer; the documented fallback is a loopback `rumqttc`
//! client (the pattern already used by `test-kit`/`test-e2e`), which is known-working regardless.

mod prelude;
mod types;

mod impl_broker_config;
mod impl_broker_publisher;
mod impl_broker_subscriber;
mod impl_connect_auth;
mod impl_local_broker;

pub use types::*;

use prelude::*;

/// Build an rumqttd CONNECT authorizer routed through the `lib-security` provider seam.
///
/// The MQTT CONNECT `password` field carries a signed/verifiable token; `username` must equal the
/// verified claims' subject (so a client cannot present a valid token for a different identity
/// under its own name). Verification runs entirely through [`TokenVerifier::verify_token`] against
/// `trusted_key` under a [`TokenPolicy`] scoped to `issuer`/`audience` — there is no hardcoded
/// credential anywhere in this path; the only trust root is `trusted_key`, supplied by the caller
/// from a real key/token backend.
///
/// **Fail-closed by construction:** any `Err` from `verify_token` — including
/// [`lib_security::SecurityError::NotImplemented`] from an unbound backend — denies the
/// connection. A concrete software `TokenVerifier` now ships in
/// `lib_security::backends::software` (default features; see `SoftwareTokenVerifier`) —
/// the wolfCrypt backend still implements none of it (see `lib-security/src/backends/
/// wolfcrypt.rs`). Wiring the gate against the trait (not a concrete backend) means binding a
/// different backend here is a drop-in, not a rewrite.
///
/// **Composability with mTLS:** a future TLS-terminated client-certificate identity can be reduced
/// to the same `(subject, issuer, audience, expires_at)` shape [`TokenClaims`](lib_security::TokenClaims)
/// already carries and fed through the same `TokenVerifier`-based gate without changing this call
/// site — mTLS is the natural follow-on, not implemented here.
pub fn seam_auth<V>(
    verifier: Arc<V>,
    trusted_key: Vec<u8>,
    issuer: String,
    audience: String,
) -> ConnectAuth
where
    V: TokenVerifier + Send + Sync + 'static,
{
    let handler: rumqttd::AuthHandler = Arc::new(move |client_id, username, password| {
        let now_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let policy = TokenPolicy {
            issuer: &issuer,
            audience: &audience,
            now_unix,
        };
        match verifier.verify_token(password.as_bytes(), &trusted_key, &policy) {
            Ok(claims) => claims.subject.as_str() == username,
            Err(e) => {
                log::warn!(
                    "mqtt CONNECT auth denied for client {client_id} (user {username}): {e}"
                );
                false
            }
        }
    });
    ConnectAuth(handler)
}

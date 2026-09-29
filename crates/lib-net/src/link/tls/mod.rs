//! A TLS record-layer stage over a [`crate::link::NetStack`] TCP socket (rustls 0.22, non-FIPS tier).
//!
//! The stage owns the TLS state machine only — it never touches the socket directly. The caller
//! pumps bytes read from `NetStack::tcp(handle).recv_slice(..)` in via [`crate::link::tls::TlsStage::feed_incoming`]
//! and bytes to write via `send_slice(..)` out of [`crate::link::tls::TlsStage::drain_outgoing`] on every poll. That
//! seam keeps the stage testable against a raw byte vector with no `NetStack` involved at all (see
//! `lib-link/tests/tls_stage.rs`), and is the seam the owned MQTT client
//! ([`crate::link::mqtt_client`]) rides for an authenticated, confidential channel.
//!
//! Feature `tls` (implies `std` — rustls needs alloc); not part of the no_std MCU build.
//!
//! ## rustls 0.22 API notes
//!
//! 0.22 replaced 0.21's `with_safe_defaults()` builder entry point with an explicit crypto
//! *provider* (here, `ring`, selected via the `ring` cargo feature): `ClientConfig::builder()` /
//! `ServerConfig::builder()` pick up `crypto::ring::default_provider()` automatically as long as
//! exactly one provider feature is enabled. The connection types (`ClientConnection`,
//! `ServerConnection`) are driven with the same four-method protocol either way:
//! `read_tls`/`process_new_packets` to feed incoming bytes in, `write_tls` to drain outgoing bytes,
//! and `reader()`/`writer()` (`std::io::Read`/`Write`) for application plaintext. This module wraps
//! both connection kinds in the crate's own `rustls::Connection` enum so `TlsStage` doesn't need to
//! duplicate the read/write plumbing per role.
//!
//! ## Server identity
//!
//! `new_server()` takes its certificate chain + signing key through [`crate::link::tls_identity`]'s
//! [`crate::link::tls_identity::TlsIdentity`] seam rather than reading cert/key bytes inline — see that module's docs for why.
//! Neither this module nor [`crate::link::tls_identity`] embeds or generates cert/key material itself:
//! every identity is caller-supplied, built via
//! [`SoftwareTlsIdentity::from_der`](crate::link::tls_identity::SoftwareTlsIdentity::from_der).
//! `new_client()`'s trust anchor is likewise caller-supplied — exactly one root certificate, passed
//! in by the caller (this crate ships no system trust store; the trust boundary is a single
//! explicit root, not an identity — out of scope for the identity seam so far). This workspace's
//! tests source that material from `test_kit::tls::generate_bringup_identity`, which mints a
//! fresh, self-signed, process-local identity (rcgen, never written to disk or checked in) valid
//! for `sentient-rs.local` and `broker.sentient-rs.local` — the two names the test suite connects
//! to. This replaces a formerly-committed fixed pair at `lib-link/certs/bringup_{cert,key}.der`;
//! see `docs/security.md`'s rotation note for why that pair is retired.

mod prelude;
mod types;
pub use types::*;

mod impl_tls_stage;

/// One TLS record/read chunk processed per call (host/std path only; this module is `tls`-feature
/// gated and not reachable from the no_std MCU build).
pub const TLS_CHUNK: usize = 4096;

//! Ephemeral TLS bring-up/test identity generation (`rcgen`, `ring` backend).
//!
//! Replaces a formerly-committed private key: `lib-link/certs/bringup_{cert,key}.der` used to be
//! a fixed, checked-in self-signed cert/key pair, `include_bytes!`'d straight into `lib-link`'s
//! `tls` module — a private key living in git history is a stock finding in any audit, so it's
//! gone (see `docs/security.md`'s rotation note: treat that old pair as revoked). `lib_net::link::tls`
//! and `lib_net::link::tls_identity` now take cert/key/root material as plain constructor parameters
//! (`TlsStage::new_client`'s `trusted_root`, `SoftwareTlsIdentity::from_der`) instead of embedding
//! anything, and this module is where this workspace's tests get that material from: a fresh,
//! self-signed identity, generated in-process, every process, never written to disk or checked
//! in.
//!
//! The generated leaf is valid for `sentient-rs.local` and `broker.sentient-rs.local` — the two
//! names `test-e2e`'s TLS suite dials — with `rcgen`'s default `not_before` (1975-01-01, safely
//! in the past for every real clock this ever runs against).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod types;

pub use types::*;

use rcgen::{generate_simple_self_signed, CertifiedKey};

/// The two subject alternative names this workspace's TLS tests dial.
pub const BRINGUP_SAN_CLIENT_NAME: &str = "sentient-rs.local";
pub const BRINGUP_SAN_BROKER_NAME: &str = "broker.sentient-rs.local";

/// Generate a fresh, self-signed identity valid for [`BRINGUP_SAN_CLIENT_NAME`] and
/// [`BRINGUP_SAN_BROKER_NAME`]. A new identity every call (and every process) — nothing here is
/// cached or reused across processes, so there is never a private key at rest on disk or in git
/// history.
pub fn generate_bringup_identity() -> GeneratedTlsIdentity {
    let CertifiedKey { cert, key_pair } = generate_simple_self_signed(vec![
        BRINGUP_SAN_CLIENT_NAME.to_string(),
        BRINGUP_SAN_BROKER_NAME.to_string(),
    ])
    .expect("a fixed, hardcoded SAN list always generates a valid self-signed certificate");
    GeneratedTlsIdentity {
        cert_der: cert.der().to_vec(),
        key_der: key_pair.serialize_der(),
    }
}

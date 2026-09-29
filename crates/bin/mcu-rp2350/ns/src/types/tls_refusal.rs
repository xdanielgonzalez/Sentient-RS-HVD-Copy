//! [`TlsRefusal`] — why the TLS interposition stage (`boot_tls.rs`, feature `tls`) turned an
//! accepted TCP connection away. Every variant is a refusal; there is no "accepted" variant since
//! this crate cannot complete a handshake yet (ADR 0013 §7, WP-6).
//!
//! Kept as an enum, not a unit struct, so the eventual real stage (`lib_tls::TlsServerStage`, ADR
//! 0013 §1) can add refusal reasons without reshaping `main.rs`'s `accept_source` call site.

/// Why an accepted TCP connection was refused before it could reach the MQTT broker pump.
///
/// This is a diagnostic value only: the interposition point's contract is that ANY value of this
/// type means "the socket was aborted and `None` was returned to the pump". The variant selects
/// the RTT line, never the outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TlsRefusal {
    /// No `lib_tls::SecureProvider` exists on this chip, so the TLS server stage cannot even be
    /// **constructed** — never mind driven.
    ///
    /// Two independent facts, either of which alone would force this refusal:
    /// - `crates/lib-tls` is a fail-closed skeleton: every `TlsServerStage` operation returns
    ///   `TlsError::Unimplemented` and `is_handshaking()` is permanently `true`, so no handshake
    ///   can complete (ADR 0013 §7, WP-3).
    /// - The Secure-world backends the stage would route through — `TrustStore`, `KeyAgreement`,
    ///   `KeyDerivation`, `RecordCipher` (ADR 0013 §1) — have no implementation, no `sv_*` veneer
    ///   entry points, no provisioned identity key, and an S-world RNG that is still
    ///   `InsecureTestRng` (ADR 0013 §7, WP-4/WP-5).
    ///
    /// Until both land this is the ONLY variant, and it is reached on every single connection.
    NoSecureProvider,
}

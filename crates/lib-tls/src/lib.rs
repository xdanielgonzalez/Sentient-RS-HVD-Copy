//! `lib-tls` — the rustls glue for the device's MQTT TLS server leg.
//!
//! **Status: SKELETON. Nothing here performs cryptography, and nothing here can authorize a peer.**
//! Every security-relevant operation returns [`TlsError::Unimplemented`]. This crate exists so that
//! ADR [0013][adr]'s transport-security design has a *named* landing site — a set of holes with
//! signatures, doc comments and call sites — rather than an architecture question that has to be
//! re-opened on the next chip. It is deliberately shaped, not deliberately empty.
//!
//! [adr]: ../../../docs/decisions/0013-transport-security-trust-store.md
//!
//! # Why this crate is a skeleton and not an implementation
//!
//! An STM32H5 migration is live on FIPS grounds (ADR 0013 §Context). Implementing the TLS stack
//! against the RP2350 would be thrown away — but the *seam* transfers, because Non-Secure code
//! depends on `lib-security` traits and never on the chip. So the seam is what gets written now.
//!
//! # The decision record, because this is where the next implementer starts
//!
//! ## rustls is the only option, and it wins by being the last one standing
//!
//! It is the only `no_std` stack that can be a TLS **server** with **client-certificate
//! verification** (ADR 0013 §3 and Appendix A). The alternatives each fail on one hard fact:
//!
//! * **embedded-tls** — the crate the embassy ecosystem reaches for — is **client-only**. Server
//!   support is issue **#51, open since 2022-06-26**, with no maintainer commitment; no `TlsServer`
//!   type exists. Independently, it **cannot hold a CA store or present a certificate at all**: the
//!   whole of `TlsConfig` is `new` / `enable_rsa_signatures` / `with_server_name` / `with_alpn` /
//!   `with_max_fragment_length` / `reset_max_fragment_length` / `with_psk`. Its only
//!   mutual-authentication-shaped primitive is a pre-shared symmetric key, which is a different
//!   trust model with no binding of identity to a key held in a custody boundary.
//! * **BearSSL** is the best resource fit in the survey (no `malloc` anywhere, ~20 kB code /
//!   ~25 kB RAM for a minimal server) and is killed by protocol version: **TLS 1.2 maximum**, with
//!   TLS 1.3 an unstarted roadmap item and no timeline.
//! * **wolfSSL** is the only stack with a live FIPS certificate (wolfCrypt, CMVP #4718) and has no
//!   `no_std` Rust binding — and #4718's operating-environment table has **no Cortex-M33 row**, so
//!   the certificate would not cover this chip even if the binding existed.
//! * **`rustls-rustcrypto`**, the obvious shortcut, says **"DO NOT USE THIS IN PRODUCTION"** in its
//!   own README and still requires `std`.
//!
//! ## `alloc` is mandatory, and it is the largest doctrinal cost in the design
//!
//! rustls 0.23's own release notes: *"`alloc` continues to be required."* A heap therefore enters a
//! previously allocation-free Non-Secure image. The mitigation is a bounded `embedded-alloc` heap
//! over a **dedicated static arena** (so `.bss` accounting stays honest), a **measured high-water
//! mark**, and a long-soak connect/disconnect HIL test — fragmentation over a multi-week uptime is
//! exactly what the memory arithmetic does not capture (ADR 0013 open question 12). An allocation
//! failure is a reset, not a graceful degradation. That is accepted and stated rather than hidden.
//!
//! ## Neither stock crypto provider works, so the provider is hand-built through the seam
//!
//! `ring` **explicitly declined Thumb support** — issue **#813, closed as not planned**. `aws-lc-rs`,
//! rustls's only FIPS-capable provider, is not a bare-metal target. A rustls `CryptoProvider` must
//! therefore be hand-built. Read the other way, that constraint is what makes this architecture
//! possible at all: a hand-built provider is a provider that can route **every** primitive through
//! the `lib-security` seam into the Secure world. The obstacle and the design point the same way.
//! See [`SeamCryptoProvider`].
//!
//! ## Client auth is pinned SPKI, not PKI — and that is what resolves the absent-clock problem
//!
//! The client sends an ordinary X.509 certificate; [`PinnedSpkiClientVerifier`] extracts the
//! `SubjectPublicKeyInfo`, hashes it, and matches it against a short list of pinned SPKI hashes held
//! by the `TrustStore`. **No chain construction. No `notBefore` / `notAfter`. No webpki.** The
//! device has no real-time clock, and pinning has no validity window to check, so the absence of
//! trusted time stops being a gap and becomes a consequence of the trust model.
//!
//! Stated honestly, because the cost is real: this is **pinning, not PKI**. There is **no CRL, no
//! OCSP, and no expiry**. Rotation is a signed configuration push, and that is the *entire*
//! revocation story. For a broker with three known clients that is the correct trade; at a hundred
//! clients it is not, and ADR 0013's revisit triggers say so.
//!
//! The coupling runs the other way too, and it is a trap worth naming: [`SeamTimeProvider`] is only
//! safe to leave shallow **while the pinning model holds**. Any later switch to chain validation
//! makes the clock load-bearing, and must not silently inherit a stub.
//!
//! # What is here
//!
//! | Type | Rustls counterpart it will become | State |
//! |---|---|---|
//! | [`SecureProvider`] | — (an aggregation over the `lib-security` seam) | **real** — a blanket impl, no new methods |
//! | [`SeamCryptoProvider`] | `rustls::crypto::CryptoProvider` | stub, fails closed |
//! | [`PinnedSpkiClientVerifier`] | `rustls::server::danger::ClientCertVerifier` | stub, fails closed |
//! | [`SeamTimeProvider`] | `rustls::time_provider::TimeProvider` | stub, fails closed |
//! | [`TlsServerStage`] | wraps `rustls::unbuffered::UnbufferedServerConnection` | stub, fails closed |
//!
//! # The fail-closed rule this crate is built on
//!
//! A stub that returns `Ok(())` from a verification is worse than no function at all: it looks
//! implemented, it passes tests, and it silently authorizes everyone. **There is no code path in
//! this crate that returns a success, authorized, verified or handshake-complete result.** The
//! matching test suite (`tests/fail_closed.rs`) asserts that property method by method, so it
//! cannot be regressed quietly.

#![no_std]
#![forbid(unsafe_code)]

// ADR 0013 §3: rustls requires `alloc`, so this crate is `no_std` **+ alloc** rather than the
// alloc-free `no_std` the rest of the MCU path holds to. Declared from the start so the doctrinal
// cost is visible in the crate root rather than arriving quietly with the rustls dependency, and so
// a consumer wiring the bounded `embedded-alloc` arena knows it is required before WP-3 lands.
extern crate alloc;

mod prelude;
mod types;

mod impl_pinned_spki_client_verifier;
mod impl_seam_crypto_provider;
mod impl_seam_time_provider;
mod impl_secure_provider;
mod impl_tls_error;
mod impl_tls_server_stage;

pub use types::*;

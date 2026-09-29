//! The provider seam: the trait contracts every backend (software / FIPS / secure-world) implements.
//!
//! These traits are the cryptographic boundary. Two invariants they encode:
//! 1. **Opaque key custody.** A private key is a [`KeyHandle`], never bytes. [`KeyStore`] can sign
//!    *with* a handle and export a *public* key, but there is deliberately no `export_private` — so
//!    a secure-world backend can hold the key in hardware and a software backend still cannot leak
//!    it by accident. The type system, not discipline, enforces no private-key export.
//!    [`SessionCipher`] applies the same doctrine to symmetric session keys: a key is
//!    [`establish`](SessionCipher::establish)ed *inside* the store and referenced by [`KeyHandle`]
//!    thereafter — it is the consumer of the channel layer's `CryptoUnit` (see `lib-net`'s
//!    `CryptoStage`).
//! 2. **Fail closed.** Every verify returns `Result<()>` (or a checked value); `Ok` means *proven*,
//!    any `Err` means *deny*. A `bool` return for a verification is intentionally avoided so a
//!    caller cannot forget to check it.
//!
//! All trait methods are slice-in / bounded-out, `no_std`-friendly. Host-only surfaces (X.509
//! chains, JWT) are gated behind `feature = "std"`.
//!
//! ## The transport-security seam (ADR 0013)
//!
//! [`TrustStore`], [`KeyAgreement`], [`KeyDerivation`], [`RecordCipher`] and [`TrustedTime`] extend
//! the same two invariants to a TLS 1.3 server leg and to the DDS/TMS identity binding. They are
//! **five separate traits, deliberately not one `CryptoOps` god-trait** — sign, verify, random and
//! digest already exist above, and re-declaring them would duplicate the seam. A consumer that
//! genuinely needs all of them (`lib-tls`) takes one aggregation alias with a blanket impl rather
//! than a new trait with new methods. See
//! `docs/decisions/0013-transport-security-trust-store.md` §1.
//!
//! Each is defined so that private key material, derived secrets and traffic keys are referenced by
//! [`KeyHandle`] and **never cross the Secure/Non-Secure boundary as bytes**.

mod prelude;
mod types;

mod impl_algorithm;
mod impl_peer_identity;

pub use types::*;

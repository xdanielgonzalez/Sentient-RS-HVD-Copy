//! The server identity seam [`crate::link::tls::TlsStage`] takes its certificate/key material through.
//!
//! `TlsStage` never touches raw cert/key bytes or a concrete `KeyStore` impl — it only knows
//! [`crate::link::tls_identity::TlsIdentity`]: build me a [`rustls::sign::CertifiedKey`] (cert chain + a signer), or fail
//! closed. That indirection is the whole point of this module: swapping the identity a
//! [`crate::link::tls::TlsStage::new_server`] presents (a caller-supplied software cert today, a
//! TrustZone/secure-element-backed identity tomorrow) never requires touching `tls.rs`.
//!
//! Two implementations ship here:
//!
//! * [`crate::link::tls_identity::SoftwareTlsIdentity`] — wraps a caller-supplied cert/key pair (built via
//!   [`crate::link::tls_identity::SoftwareTlsIdentity::from_der`]) and signs directly with the loaded private key. No
//!   cert/key bytes are embedded in this crate; this workspace's tests source them from
//!   `test_kit::tls::generate_bringup_identity`. Honestly reports `KeyCustody::SoftwareMemory`
//!   (`lib-security`'s mandatory-honesty accessor — see `KeyStore::custody`) — it is never
//!   claimed to be hardware-bound.
//! * [`crate::link::tls_identity::KeyStoreTlsIdentity`] — generic over any `KeyStore`. Signing happens *at* the store (via
//!   `KeyStore::sign` and an opaque `KeyHandle`), never by extracting private key bytes, so a
//!   secure-world backend (TrustZone, wolfCrypt) can be dropped in here without ever exposing key
//!   material to this crate. A concrete software `KeyStore` now ships in
//!   `lib_security::backends::software` (default features; see `SoftwareKeyStore`) and
//!   implements `sign`/`generate` for real, so an identity built on it works today. The
//!   wolfCrypt backend's `KeyStore::sign`/`generate` (`lib-security/src/backends/
//!   wolfcrypt.rs`) are still fail-closed `NotImplemented` stubs (not yet bound to the system
//!   wolfSSL library), so an identity built on `WolfKeyStore` will fail *at handshake time* if
//!   actually asked to sign — the type-level seam is real, ahead of that backend landing.

mod prelude;
mod types;

mod impl_always_resolves_identity;
mod impl_key_store_signer;
mod impl_key_store_signing_key;
mod impl_key_store_tls_identity;
mod impl_software_tls_identity;

pub use types::*;

use prelude::*;

/// Wrap a [`TlsIdentity`] into the [`ResolvesServerCert`] rustls' `ServerConfig` builder wants.
pub(crate) fn resolver_for(
    identity: &dyn TlsIdentity,
) -> Result<Arc<dyn ResolvesServerCert>, TlsError> {
    let certified_key = identity.certified_key()?;
    Ok(Arc::new(AlwaysResolvesIdentity(certified_key)))
}

/// Map a `lib-security` [`Algorithm`] to the rustls [`SignatureScheme`] it corresponds to.
/// `None` for an algorithm rustls has no scheme for (there is none today; kept fallible, with an
/// explicit wildcard for `Algorithm`'s `#[non_exhaustive]` future variants, rather than
/// exhaustive-panicking, so an unmapped algorithm fails closed instead of panicking here).
fn scheme_for(algorithm: Algorithm) -> Option<SignatureScheme> {
    match algorithm {
        Algorithm::Ed25519 => Some(SignatureScheme::ED25519),
        Algorithm::EcdsaP256Sha256 => Some(SignatureScheme::ECDSA_NISTP256_SHA256),
        Algorithm::EcdsaP384Sha384 => Some(SignatureScheme::ECDSA_NISTP384_SHA384),
        Algorithm::RsaPkcs1Sha256 => Some(SignatureScheme::RSA_PKCS1_SHA256),
        _ => None,
    }
}

/// The [`SignatureAlgorithm`] family a `lib-security` [`Algorithm`] belongs to (rustls'
/// `SignatureScheme::sign` equivalent, which is crate-private there — this is our own copy of the
/// same, small, RFC-fixed mapping). `Unknown(0)` for `Algorithm`'s `#[non_exhaustive]` future
/// variants — [`scheme_for`] already refuses those before a `Signer` is ever built, so this arm is
/// unreachable in practice; it exists only so the match stays exhaustive without panicking.
fn sig_algorithm_for(algorithm: Algorithm) -> SignatureAlgorithm {
    match algorithm {
        Algorithm::Ed25519 => SignatureAlgorithm::ED25519,
        Algorithm::EcdsaP256Sha256 | Algorithm::EcdsaP384Sha384 => SignatureAlgorithm::ECDSA,
        Algorithm::RsaPkcs1Sha256 => SignatureAlgorithm::RSA,
        _ => SignatureAlgorithm::Unknown(0),
    }
}

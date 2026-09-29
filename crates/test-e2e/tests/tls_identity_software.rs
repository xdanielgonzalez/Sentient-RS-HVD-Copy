//! Exercises [`KeyStoreTlsIdentity`] against the REAL `lib-security` software `KeyStore` — the
//! concrete backend PRs #92 (this identity seam) and #93 (the broker's `TokenVerifier` seam, see
//! `lib-mqtt/tests/token_auth_software.rs`) wired but could not yet fulfill (the seam's
//! `KeyStore::sign`/`generate` were fail-closed `NotImplemented` stubs — see `tls_identity.rs`'s
//! module docs).
//!
//! What this proves, end to end, through the REAL production call path:
//! * [`lib_security::backends::software::SoftwareKeyStore`] generates a real ECDSA P-256 key.
//! * [`KeyStoreTlsIdentity::certified_key`] builds a [`rustls::sign::CertifiedKey`] whose
//!   [`rustls::sign::SigningKey`] is [`KeyStoreSigner`] (private, but exercised here) — i.e. rustls'
//!   own `Signer` trait, not a bypass — and calling it never touches key material directly; it goes
//!   through [`KeyStore::sign`] with the opaque handle.
//! * The signature that call produces is byte-for-byte what
//!   `lib_security::Verifier`'s software backend
//!   ([`lib_security::backends::software::SoftwareVerifier`]) accepts as valid over the same
//!   message and the store's own exported public key — proving this is a real, verifiable ECDSA
//!   signature, not a placeholder.
//!
//! A full rustls handshake using this identity as the server is NOT exercised here: `TlsStage::
//! new_client` (see `tls.rs`) hardcodes its one trust root to the embedded bring-up certificate, so
//! a `KeyStoreTlsIdentity`-backed server (a different key, with no matching certificate chain in
//! this test) cannot be verified against it without also minting a matching leaf certificate — out
//! of scope for proving the KeyStore/TokenVerifier backends real. This is the documented "at
//! minimum" fallback: KeyStore signs, `Verifier` verifies.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use rustls::pki_types::CertificateDer;
use rustls::SignatureScheme;

use lib_net::link::tls_identity::{KeyStoreTlsIdentity, TlsIdentity};
use lib_security::backends::software::{InsecureTestRng, SoftwareKeyStore, SoftwareVerifier};
use lib_security::{Algorithm, KeyCustody, KeyStore, Verifier};

#[test]
fn key_store_tls_identity_signs_with_real_software_backend_and_verifies() {
    let mut store = SoftwareKeyStore::new(InsecureTestRng::new(0xC0FF_EE00_2026));
    let handle = store
        .generate(Algorithm::EcdsaP256Sha256)
        .expect("software KeyStore generates a real ECDSA P-256 key");

    let mut public_key = [0u8; 65];
    let pk_len = store
        .export_public(handle, &mut public_key)
        .expect("export_public reads back the generated key's public half");

    // A placeholder cert chain — this test proves signing/verification through the identity seam,
    // not X.509 chain building (out of scope; see module doc).
    let placeholder_chain = vec![CertificateDer::from(vec![0x30, 0x00])];

    let identity = KeyStoreTlsIdentity::new(
        Arc::new(store),
        handle,
        Algorithm::EcdsaP256Sha256,
        placeholder_chain,
    );

    // Mandatory honesty accessor: a KeyStoreTlsIdentity over the software backend must report
    // SoftwareMemory custody, delegated straight through from `KeyStore::custody`.
    assert_eq!(identity.custody(), KeyCustody::SoftwareMemory);

    let certified_key = identity
        .certified_key()
        .expect("certified_key builds successfully once the KeyStore backend is real");

    let signer = certified_key
        .key
        .choose_scheme(&[SignatureScheme::ECDSA_NISTP256_SHA256])
        .expect("the software backend's algorithm maps to a scheme rustls offers");

    let message = b"a real TLS handshake transcript hash would go here";
    let signature = signer
        .sign(message)
        .expect("KeyStore::sign (real backend) produces a signature, not NotImplemented");

    // Independent verification through `lib_security::Verifier`'s software backend — proving
    // the signature rustls' `Signer` trait produced is a genuine, checkable ECDSA P-256 signature
    // over `message` and the store's own exported public key.
    SoftwareVerifier
        .verify(
            Algorithm::EcdsaP256Sha256,
            &public_key[..pk_len],
            message,
            &signature,
        )
        .expect("the KeyStore-produced signature verifies against the KeyStore's own public key");

    // And, fail-closed: a different message must NOT verify against the same signature.
    let verify_wrong_message = SoftwareVerifier.verify(
        Algorithm::EcdsaP256Sha256,
        &public_key[..pk_len],
        b"a different, unsigned message",
        &signature,
    );
    assert!(verify_wrong_message.is_err());
}

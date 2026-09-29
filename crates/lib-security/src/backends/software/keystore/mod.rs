//! The software [`KeyStore`]: a bounded in-memory arena of ECDSA P-256 keys, keyed by opaque
//! [`KeyHandle`].
//!
//! [`SoftwareKeyStore`] holds up to [`ARENA_CAP`] `ecdsa::SigningKey` values in a `heapless::Vec`
//! (bounded, no allocation). [`KeyHandle`] is the key's index in that arena — opaque to callers,
//! meaningless outside this store, exactly as [`crate::traits::KeyStore`] documents. There is no
//! `remove` and no `export_private`: the only way key material leaves this module is a *public* key
//! ([`KeyStore::export_public`]) or a *signature* ([`KeyStore::sign`]) — never the private scalar.
//! Private keys are zeroized on drop by `ecdsa::SigningKey`'s own `Drop` impl (see the RustCrypto
//! source: it zeroizes the secret scalar).

mod prelude;
mod types;

mod impl_software_key_store;

pub use types::*;

#[cfg(test)]
use prelude::*;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::backends::software::ecdsa::{SoftwareVerifier, PUBLIC_KEY_LEN, SIGNATURE_LEN};
    use crate::backends::software::rng::InsecureTestRng;
    use crate::traits::Verifier;

    fn store() -> SoftwareKeyStore<InsecureTestRng> {
        SoftwareKeyStore::new(InsecureTestRng::new(42))
    }

    #[test]
    fn reports_software_custody() {
        assert_eq!(store().custody(), KeyCustody::SoftwareMemory);
    }

    #[test]
    fn generate_rejects_unsupported_algorithm() {
        let mut ks = store();
        assert_eq!(
            ks.generate(Algorithm::Ed25519),
            Err(SecurityError::NotImplemented)
        );
    }

    #[test]
    fn unknown_handle_is_key_not_found() {
        let ks = store();
        let mut out = [0u8; PUBLIC_KEY_LEN];
        assert_eq!(
            ks.export_public(KeyHandle(0), &mut out),
            Err(SecurityError::KeyNotFound)
        );
    }

    #[test]
    fn generate_sign_export_verify_roundtrip() {
        let mut ks = store();
        let handle = ks.generate(Algorithm::EcdsaP256Sha256).unwrap();

        let mut public_key = [0u8; PUBLIC_KEY_LEN];
        let pk_len = ks.export_public(handle, &mut public_key).unwrap();
        assert_eq!(pk_len, PUBLIC_KEY_LEN);

        let message = b"real signature, real backend";
        let mut signature = [0u8; SIGNATURE_LEN];
        let sig_len = ks.sign(handle, message, &mut signature).unwrap();
        assert_eq!(sig_len, SIGNATURE_LEN);

        SoftwareVerifier
            .verify(
                Algorithm::EcdsaP256Sha256,
                &public_key[..pk_len],
                message,
                &signature[..sig_len],
            )
            .unwrap();
    }

    #[test]
    fn tampered_signature_fails_closed() {
        let mut ks = store();
        let handle = ks.generate(Algorithm::EcdsaP256Sha256).unwrap();
        let mut public_key = [0u8; PUBLIC_KEY_LEN];
        ks.export_public(handle, &mut public_key).unwrap();

        let message = b"do not trust a flipped bit";
        let mut signature = [0u8; SIGNATURE_LEN];
        ks.sign(handle, message, &mut signature).unwrap();
        signature[0] ^= 0xFF;

        assert_eq!(
            SoftwareVerifier.verify(Algorithm::EcdsaP256Sha256, &public_key, message, &signature),
            Err(SecurityError::VerificationFailed)
        );
    }

    #[test]
    fn wrong_message_fails_closed() {
        let mut ks = store();
        let handle = ks.generate(Algorithm::EcdsaP256Sha256).unwrap();
        let mut public_key = [0u8; PUBLIC_KEY_LEN];
        ks.export_public(handle, &mut public_key).unwrap();
        let mut signature = [0u8; SIGNATURE_LEN];
        ks.sign(handle, b"message A", &mut signature).unwrap();

        assert_eq!(
            SoftwareVerifier.verify(
                Algorithm::EcdsaP256Sha256,
                &public_key,
                b"message B",
                &signature
            ),
            Err(SecurityError::VerificationFailed)
        );
    }

    #[test]
    fn import_private_roundtrips_with_generated_key() {
        // White-box: reach into the arena to get raw scalar bytes for a generated key (there is
        // deliberately no public `export_private` — this test uses module-internal access, not the
        // trait surface, precisely BECAUSE the trait must never offer this).
        let mut ks = store();
        let handle = ks.generate(Algorithm::EcdsaP256Sha256).unwrap();
        let scalar_bytes = ks.get(handle).unwrap().to_bytes();

        let mut ks2 = store();
        let imported = ks2
            .import_private(Algorithm::EcdsaP256Sha256, scalar_bytes.as_ref())
            .unwrap();

        let mut pk1 = [0u8; PUBLIC_KEY_LEN];
        let mut pk2 = [0u8; PUBLIC_KEY_LEN];
        ks.export_public(handle, &mut pk1).unwrap();
        ks2.export_public(imported, &mut pk2).unwrap();
        assert_eq!(
            pk1, pk2,
            "importing a key's raw scalar must reproduce the same public key"
        );
    }

    #[test]
    fn import_private_rejects_malformed_bytes() {
        let mut ks = store();
        assert_eq!(
            ks.import_private(Algorithm::EcdsaP256Sha256, &[0u8; 3]),
            Err(SecurityError::Malformed)
        );
    }

    #[test]
    fn arena_capacity_is_bounded() {
        let mut ks = store();
        for _ in 0..ARENA_CAP {
            ks.generate(Algorithm::EcdsaP256Sha256).unwrap();
        }
        assert_eq!(
            ks.generate(Algorithm::EcdsaP256Sha256),
            Err(SecurityError::Backend),
            "the arena must fail closed once full, never silently evict or grow"
        );
    }
}

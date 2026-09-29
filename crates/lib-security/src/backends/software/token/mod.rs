//! The software [`TokenVerifier`]: a minimal signed-claims token over ECDSA P-256.
//!
//! Wire format: the pipe-delimited claims text `subject|issuer|audience|expires_at` (matching
//! [`TokenClaims`]'s fields, in order) followed immediately by the ECDSA P-256 signature (`r ‖ s`)
//! over that claims text, **hex-encoded** — no JSON, no base64, fully `no_std`/no-`alloc` (a small
//! hand-rolled hex codec, not a new dependency), and no ambiguity about what was signed (the whole
//! token minus the fixed trailing hex length IS the signed message, after decoding).
//!
//! **Why hex, not raw signature bytes:** this crate's [`TokenVerifier`] trait takes `token: &[u8]`
//! generically, but the one real production carrier in this workspace —
//! `lib_mqtt::broker::seam_auth`'s MQTT CONNECT password — is typed `String` end-to-end through
//! `rumqttc`/`rumqttd` (`AuthPass = String`, `set_credentials<P: Into<String>>`), which forces valid
//! UTF-8. A raw 64-byte ECDSA signature is essentially never valid UTF-8. Hex-encoding the signature
//! (doubling it to 128 ASCII bytes) makes the WHOLE token guaranteed-valid UTF-8, so it round-trips
//! through that real transport unchanged — see `lib-mqtt/tests/token_auth_software.rs`, which
//! sends a real token as an MQTT password over a real network client. This is a genuine integration
//! constraint discovered wiring this backend to its real consumer, not an inherent requirement of
//! the `TokenVerifier` trait itself (which is transport-agnostic).
//!
//! [`sign_token`] is the paired signer used by tests (and any real issuer) to mint a token
//! [`SoftwareTokenVerifier::verify_token`] accepts — it takes a [`KeyStore`] + [`KeyHandle`], never a
//! raw signing key, so minting a token never bypasses the opaque-handle custody boundary.
//!
//! Verification order is signature-first (cryptographic proof) THEN claims content
//! (issuer/audience/expiry) — never the reverse — and every failure path denies via a distinct
//! [`SecurityError`] variant, never a silent downgrade. Subject/issuer/audience must not contain `|`
//! (the field delimiter) — [`sign_token`] refuses to mint a token whose fields would be ambiguous to
//! parse back out.

mod prelude;
mod types;

mod impl_hex_codec;
mod impl_sign_token;
mod impl_software_token_verifier;

pub use types::*;

pub use impl_sign_token::sign_token;

use impl_hex_codec::{decode_hex, encode_hex};

use super::ecdsa::SIGNATURE_LEN;

// Used only by the test module below (the production impls moved to the `impl_*` files, which carry
// their own imports); gated so the non-test lib build doesn't see them as unused.
#[cfg(test)]
use crate::error::SecurityError;
#[cfg(test)]
use crate::traits::{KeyHandle, KeyStore, TokenPolicy, TokenVerifier};

/// Max encoded length of a token's pipe-delimited claims text (three `TokenClaims` fields of up to
/// 64 bytes each, 3 `|` delimiters, plus `expires_at` as up to 20 decimal digits).
const CLAIMS_TEXT_MAX: usize = 64 * 3 + 3 + 20;

/// Hex-encoded ECDSA P-256 signature length (2 hex characters per raw byte).
const SIGNATURE_HEX_LEN: usize = SIGNATURE_LEN * 2;

/// Max total token length [`sign_token`] ever mints and [`SoftwareTokenVerifier`] ever accepts
/// (claims text + trailing hex-encoded signature). The whole token is guaranteed-valid UTF-8/ASCII.
pub const TOKEN_MAX: usize = CLAIMS_TEXT_MAX + SIGNATURE_HEX_LEN;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::backends::software::ecdsa::PUBLIC_KEY_LEN;
    use crate::backends::software::keystore::SoftwareKeyStore;
    use crate::backends::software::rng::InsecureTestRng;
    use crate::traits::Algorithm;

    const ISSUER: &str = "sentient-test-issuer";
    const AUDIENCE: &str = "lib-mqtt-broker";

    /// A fresh issuer store + key (seeded by `seed`), and its exported public key — the shape a
    /// real token issuer (or the broker-auth test in `lib-mqtt`) would hold.
    fn issuer(
        seed: u64,
    ) -> (
        SoftwareKeyStore<InsecureTestRng>,
        KeyHandle,
        [u8; PUBLIC_KEY_LEN],
    ) {
        let mut store = SoftwareKeyStore::new(InsecureTestRng::new(seed));
        let handle = store.generate(Algorithm::EcdsaP256Sha256).unwrap();
        let mut public_key = [0u8; PUBLIC_KEY_LEN];
        store.export_public(handle, &mut public_key).unwrap();
        (store, handle, public_key)
    }

    #[test]
    fn hex_roundtrips() {
        let raw: [u8; SIGNATURE_LEN] = core::array::from_fn(|i| i as u8);
        let mut hex = [0u8; SIGNATURE_HEX_LEN];
        encode_hex(&raw, &mut hex);
        assert!(hex.iter().all(|b| b.is_ascii_hexdigit()));

        let mut back = [0u8; SIGNATURE_LEN];
        decode_hex(&hex, &mut back).unwrap();
        assert_eq!(raw, back);
    }

    #[test]
    fn decode_hex_rejects_non_hex_and_wrong_length() {
        let mut out = [0u8; 2];
        assert_eq!(decode_hex(b"zz11", &mut out), Err(SecurityError::Malformed));
        assert_eq!(decode_hex(b"aa", &mut out), Err(SecurityError::Malformed)); // too short for out
    }

    #[test]
    fn minted_token_is_ascii() {
        let (store, handle, _pk) = issuer(1);
        let mut token = [0u8; TOKEN_MAX];
        let len = sign_token(
            &store,
            handle,
            "d",
            ISSUER,
            AUDIENCE,
            9_999_999_999,
            &mut token,
        )
        .unwrap();
        assert!(
            token[..len].is_ascii(),
            "the whole token must be ASCII/UTF-8-safe (e.g. to survive an MQTT String password field)"
        );
    }

    #[test]
    fn valid_token_is_accepted() {
        let (store, handle, public_key) = issuer(7);

        let mut token = [0u8; TOKEN_MAX];
        let len = sign_token(
            &store,
            handle,
            "device-1",
            ISSUER,
            AUDIENCE,
            9_999_999_999,
            &mut token,
        )
        .unwrap();

        let policy = TokenPolicy {
            issuer: ISSUER,
            audience: AUDIENCE,
            now_unix: 1_000,
        };
        let claims = SoftwareTokenVerifier
            .verify_token(&token[..len], &public_key, &policy)
            .unwrap();
        assert_eq!(claims.subject.as_str(), "device-1");
        assert_eq!(claims.issuer.as_str(), ISSUER);
        assert_eq!(claims.audience.as_str(), AUDIENCE);
        assert_eq!(claims.expires_at, 9_999_999_999);
    }

    #[test]
    fn tampered_token_is_rejected() {
        let (store, handle, public_key) = issuer(7);
        let mut token = [0u8; TOKEN_MAX];
        let len = sign_token(
            &store,
            handle,
            "device-1",
            ISSUER,
            AUDIENCE,
            9_999_999_999,
            &mut token,
        )
        .unwrap();
        // Flip a hex digit inside the signature (the last SIGNATURE_HEX_LEN bytes) — still valid
        // hex (any digit XORed with 0x01 that stays a hex char), just a different signature.
        let last = len - 1;
        token[last] = if token[last] == b'0' { b'1' } else { b'0' };

        let policy = TokenPolicy {
            issuer: ISSUER,
            audience: AUDIENCE,
            now_unix: 1_000,
        };
        assert!(matches!(
            SoftwareTokenVerifier.verify_token(&token[..len], &public_key, &policy),
            Err(SecurityError::VerificationFailed)
        ));
    }

    #[test]
    fn token_signed_by_untrusted_key_is_rejected() {
        let (store, handle, _own_public_key) = issuer(7);
        let (_other_store, _other_handle, untrusted_public_key) = issuer(99);

        let mut token = [0u8; TOKEN_MAX];
        let len = sign_token(
            &store,
            handle,
            "device-1",
            ISSUER,
            AUDIENCE,
            9_999_999_999,
            &mut token,
        )
        .unwrap();

        let policy = TokenPolicy {
            issuer: ISSUER,
            audience: AUDIENCE,
            now_unix: 1_000,
        };
        assert!(matches!(
            SoftwareTokenVerifier.verify_token(&token[..len], &untrusted_public_key, &policy),
            Err(SecurityError::VerificationFailed)
        ));
    }

    #[test]
    fn wrong_issuer_or_audience_is_identity_mismatch() {
        let (store, handle, public_key) = issuer(7);
        let mut token = [0u8; TOKEN_MAX];
        let len = sign_token(
            &store,
            handle,
            "device-1",
            ISSUER,
            AUDIENCE,
            9_999_999_999,
            &mut token,
        )
        .unwrap();

        let wrong_issuer = TokenPolicy {
            issuer: "someone-else",
            audience: AUDIENCE,
            now_unix: 1_000,
        };
        assert!(matches!(
            SoftwareTokenVerifier.verify_token(&token[..len], &public_key, &wrong_issuer),
            Err(SecurityError::IdentityMismatch)
        ));

        let wrong_audience = TokenPolicy {
            issuer: ISSUER,
            audience: "someone-elses-broker",
            now_unix: 1_000,
        };
        assert!(matches!(
            SoftwareTokenVerifier.verify_token(&token[..len], &public_key, &wrong_audience),
            Err(SecurityError::IdentityMismatch)
        ));
    }

    #[test]
    fn expired_token_is_rejected() {
        let (store, handle, public_key) = issuer(7);
        let mut token = [0u8; TOKEN_MAX];
        let len = sign_token(
            &store, handle, "device-1", ISSUER, AUDIENCE, 1_000, &mut token,
        )
        .unwrap();

        let policy = TokenPolicy {
            issuer: ISSUER,
            audience: AUDIENCE,
            now_unix: 1_000, // expires_at <= now_unix -> expired
        };
        assert!(matches!(
            SoftwareTokenVerifier.verify_token(&token[..len], &public_key, &policy),
            Err(SecurityError::Expired)
        ));
    }

    #[test]
    fn sign_token_rejects_delimiter_in_field() {
        let (store, handle, _public_key) = issuer(7);
        let mut token = [0u8; TOKEN_MAX];
        assert_eq!(
            sign_token(
                &store,
                handle,
                "device|evil",
                ISSUER,
                AUDIENCE,
                9_999_999_999,
                &mut token
            ),
            Err(SecurityError::Malformed)
        );
    }

    #[test]
    fn malformed_token_is_rejected() {
        let (_store, _handle, public_key) = issuer(7);
        let policy = TokenPolicy {
            issuer: ISSUER,
            audience: AUDIENCE,
            now_unix: 1_000,
        };
        // Shorter than one hex-encoded signature: cannot possibly carry a signature.
        assert!(matches!(
            SoftwareTokenVerifier.verify_token(&[0u8; 10], &public_key, &policy),
            Err(SecurityError::Malformed)
        ));
    }
}

//! The software [`SessionCipher`]: a bounded in-memory arena of AES-256-GCM session keys, keyed by
//! opaque [`KeyHandle`] — the consumer-facing counterpart to `lib-net`'s channel-layer `CryptoUnit`.
//!
//! [`SoftwareSessionStore`] holds up to [`SESSION_ARENA_CAP`] AES-256-GCM cipher instances (RustCrypto
//! `aes-gcm`) in a `heapless::Vec` (bounded, no allocation), each paired with its own monotonic
//! nonce counter. [`KeyHandle`] is the key's index in that arena — opaque to callers, meaningless
//! outside this store, exactly as [`crate::SessionCipher`] documents. There is no `establish`
//! counterpart that returns key bytes: the only way a key born here affects the outside world is
//! through [`seal`](crate::SessionCipher::seal)/[`open`](crate::SessionCipher::open) — never a raw
//! export. Key material lives at the `KeyCustody::SoftwareMemory` tier this store honestly reports
//! (normal process memory, not hardware-bound) — see the crate's `Cargo.toml` comment on the
//! `aes-gcm` `"zeroize"` feature for exactly what is and is not additionally scrubbed on drop.
//!
//! See [`crate::SessionCipher`]'s trait doc for the wire framing
//! (`[12-byte nonce][ciphertext][16-byte tag]`) and the nonce-reuse-hazard rationale for keeping
//! the nonce counter inside this store rather than accepting one from the caller.
//!
//! ## FIPS posture: unchanged from the rest of `backends::software`
//! This is pure-Rust AES-256-GCM run outside any FIPS 140-3 validated module — same non-FIPS
//! posture the parent module already states for [`super::SoftwareKeyStore`]. There is deliberately
//! **no** `feature = "fips"`-conditional code here: [`crate::Algorithm`] (and therefore
//! [`crate::Provider::check_approved`]/[`crate::approved_under_fips`]) covers the signature
//! algorithms [`crate::KeyStore`] selects among, not a fixed single-cipher [`crate::SessionCipher`]
//! backend — inventing a parallel approval gate for AES-256-GCM here would be new policy this crate
//! does not otherwise have. `wolfcrypt` remains the separate FIPS-*capable* path.

mod prelude;
mod types;

mod impl_software_session_store;

pub use types::*;

#[cfg(test)]
use prelude::*;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::backends::software::rng::InsecureTestRng;

    fn store() -> SoftwareSessionStore<InsecureTestRng> {
        SoftwareSessionStore::new(InsecureTestRng::new(7))
    }

    #[test]
    fn reports_software_custody() {
        assert_eq!(store().custody(), KeyCustody::SoftwareMemory);
    }

    #[test]
    fn unknown_handle_is_key_not_found() {
        let mut store = store();
        let mut buf = [0u8; 64];
        assert_eq!(
            store.seal(KeyHandle(0), b"", &mut buf, 16),
            Err(SecurityError::KeyNotFound)
        );
        assert_eq!(
            store.open(KeyHandle(0), b"", &mut buf, 32),
            Err(SecurityError::KeyNotFound)
        );
    }

    #[test]
    fn establish_beyond_arena_capacity_fails_closed() {
        let mut store = store();
        for _ in 0..SESSION_ARENA_CAP {
            store.establish().unwrap();
        }
        assert_eq!(
            store.establish(),
            Err(SecurityError::Backend),
            "the arena must fail closed once full, never silently evict or grow"
        );
    }

    #[test]
    fn seal_open_roundtrip_with_aad() {
        let mut store = store();
        let handle = store.establish().unwrap();

        let plaintext = b"session cipher round trip";
        let aad = b"channel-layer context";
        let mut buf = [0u8; 128];
        buf[..plaintext.len()].copy_from_slice(plaintext);

        let sealed_len = store.seal(handle, aad, &mut buf, plaintext.len()).unwrap();
        assert_eq!(sealed_len, plaintext.len() + NONCE_LEN + TAG_LEN);
        assert_ne!(&buf[NONCE_LEN..NONCE_LEN + plaintext.len()], plaintext);

        let opened_len = store.open(handle, aad, &mut buf, sealed_len).unwrap();
        assert_eq!(opened_len, plaintext.len());
        assert_eq!(&buf[..opened_len], plaintext);
    }

    #[test]
    fn open_rejects_wrong_aad() {
        let mut store = store();
        let handle = store.establish().unwrap();

        let plaintext = b"authenticated data matters";
        let mut buf = [0u8; 128];
        buf[..plaintext.len()].copy_from_slice(plaintext);
        let sealed_len = store
            .seal(handle, b"context A", &mut buf, plaintext.len())
            .unwrap();

        assert_eq!(
            store.open(handle, b"context B", &mut buf, sealed_len),
            Err(SecurityError::VerificationFailed)
        );
    }

    #[test]
    fn tampered_ciphertext_fails_closed_and_scrubs_buffer() {
        let mut store = store();
        let handle = store.establish().unwrap();

        let plaintext = b"do not trust a flipped bit";
        let mut buf = [0u8; 128];
        buf[..plaintext.len()].copy_from_slice(plaintext);
        let sealed_len = store.seal(handle, b"", &mut buf, plaintext.len()).unwrap();
        buf[NONCE_LEN] ^= 0xFF;

        assert_eq!(
            store.open(handle, b"", &mut buf, sealed_len),
            Err(SecurityError::VerificationFailed)
        );
        assert!(
            buf[..sealed_len].iter().all(|&b| b == 0),
            "a failed open must never leave any usable bytes behind"
        );
    }

    #[test]
    fn seal_reports_buffer_too_small() {
        let mut store = store();
        let handle = store.establish().unwrap();

        let plaintext = b"too long for this buffer";
        let mut buf = [0u8; 8]; // shorter than plaintext.len() + NONCE_LEN + TAG_LEN
        assert_eq!(
            store.seal(handle, b"", &mut buf, plaintext.len()),
            Err(SecurityError::BufferTooSmall)
        );
    }

    #[test]
    fn open_rejects_undersized_framing_as_malformed() {
        let mut store = store();
        let handle = store.establish().unwrap();
        let mut buf = [0u8; 64];
        assert_eq!(
            store.open(handle, b"", &mut buf, NONCE_LEN + TAG_LEN - 1),
            Err(SecurityError::Malformed)
        );
    }

    #[test]
    fn nonce_advances_between_seals_of_identical_plaintext() {
        let mut store = store();
        let handle = store.establish().unwrap();

        let plaintext = b"same plaintext, twice";
        let mut buf_a = [0u8; 64];
        let mut buf_b = [0u8; 64];
        buf_a[..plaintext.len()].copy_from_slice(plaintext);
        buf_b[..plaintext.len()].copy_from_slice(plaintext);

        let len_a = store
            .seal(handle, b"", &mut buf_a, plaintext.len())
            .unwrap();
        let len_b = store
            .seal(handle, b"", &mut buf_b, plaintext.len())
            .unwrap();

        assert_eq!(len_a, len_b);
        assert_ne!(
            &buf_a[..NONCE_LEN],
            &buf_b[..NONCE_LEN],
            "the per-key nonce counter must advance between seals"
        );
        assert_ne!(
            &buf_a[..len_a],
            &buf_b[..len_b],
            "identical plaintext sealed twice under an advancing nonce must not produce identical output"
        );
    }
}

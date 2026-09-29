use super::prelude::*;

use super::{encode_hex, CLAIMS_TEXT_MAX, SIGNATURE_HEX_LEN};

/// Mint a token [`SoftwareTokenVerifier::verify_token`] accepts: sign
/// `subject|issuer|audience|expires_at` with the key `handle` references inside `store`, writing
/// the wire-format token (claims text followed by the hex-encoded signature) into `out`. The paired
/// signer for [`SoftwareTokenVerifier`] — used by tests here and by any real token issuer.
///
/// Takes a [`KeyStore`] + [`KeyHandle`] — never a raw signing key — so minting a token goes through
/// the same opaque-handle custody boundary as every other signing operation in this crate; `store`
/// need not be a [`super::super::keystore::SoftwareKeyStore`] specifically, only produce an ECDSA P-256
/// signature of [`SIGNATURE_LEN`] raw bytes from `handle` (any [`KeyStore`] impl backed by a
/// different algorithm is refused — see below).
///
/// Refuses [`SecurityError::Malformed`] if any field contains the `|` delimiter (ambiguous to parse
/// back out), [`SecurityError::BufferTooSmall`] if `out` cannot hold the encoded token, and
/// [`SecurityError::NotImplemented`] if `handle` does not produce a [`SIGNATURE_LEN`]-byte raw
/// signature (i.e. is not an ECDSA P-256 key — this wire format has no room for a differently-sized
/// signature).
pub fn sign_token<K: KeyStore>(
    store: &K,
    handle: KeyHandle,
    subject: &str,
    issuer: &str,
    audience: &str,
    expires_at: u64,
    out: &mut [u8],
) -> Result<usize> {
    if subject.contains('|') || issuer.contains('|') || audience.contains('|') {
        return Err(SecurityError::Malformed);
    }

    let mut claims_text: heapless::String<CLAIMS_TEXT_MAX> = heapless::String::new();
    write!(claims_text, "{subject}|{issuer}|{audience}|{expires_at}")
        .map_err(|_| SecurityError::BufferTooSmall)?;
    let claims_bytes = claims_text.as_bytes();

    let total = claims_bytes.len() + SIGNATURE_HEX_LEN;
    if out.len() < total {
        return Err(SecurityError::BufferTooSmall);
    }

    let mut raw_sig = [0u8; SIGNATURE_LEN];
    let sig_len = store.sign(handle, claims_bytes, &mut raw_sig)?;
    if sig_len != SIGNATURE_LEN {
        return Err(SecurityError::NotImplemented);
    }

    out[..claims_bytes.len()].copy_from_slice(claims_bytes);
    encode_hex(&raw_sig, &mut out[claims_bytes.len()..total]);
    Ok(total)
}

use super::prelude::*;
use super::types::WolfVerifier;

impl Verifier for WolfVerifier {
    fn verify(
        &self,
        algorithm: Algorithm,
        public_key: &[u8],
        message: &[u8],
        signature: &[u8],
    ) -> Result<()> {
        match algorithm {
            Algorithm::Ed25519 => {
                let key = parse_key::<wolf::Ed25519Key>(public_key)?;
                let sig = parse_sig::<wolf::Ed25519Sig>(signature)?;
                verify_generic(&key, message, &sig)
            }
            Algorithm::EcdsaP256Sha256 => {
                let key = parse_key::<wolf::P256Key>(public_key)?;
                let sig = parse_sig::<wolf::P256Sig>(signature)?;
                verify_generic(&key, message, &sig)
            }
            Algorithm::EcdsaP384Sha384 => {
                let key = parse_key::<wolf::P384Key>(public_key)?;
                let sig = parse_sig::<wolf::P384Sig>(signature)?;
                verify_generic(&key, message, &sig)
            }
            Algorithm::RsaPkcs1Sha256 => {
                // WOLFSSL-API: confirmed — `rsa_pkcs1v15::VerifyingKey<H, N>` has NO `TryFrom<&[u8]>`
                // impl (unlike the ECDSA/Ed25519 keys); it is built from a DER-encoded key via
                // `from_public_der`, or from raw `(n, e)` via `from_components`. `public_key` is
                // therefore treated as a DER-encoded RSA public key. A modulus size other than 256
                // bytes (RSA-2048) fails `from_rsa`'s internal `check_modulus_size` and is reported
                // as `Malformed`, per the semantic narrowing documented in the module doc comment.
                let key = wolf::RsaKey::from_public_der(public_key)
                    .map_err(|_| SecurityError::Malformed)?;
                let sig = parse_sig::<wolf::RsaSig>(signature)?;
                verify_generic(&key, message, &sig)
            }
        }
    }
}

/// Parse a public key from raw bytes into a wolfCrypt verifying key. A malformed encoding is
/// [`SecurityError::Malformed`] (fail closed).
///
/// NOT used for RSA — `rsa_pkcs1v15::VerifyingKey<H, N>` has no `TryFrom<&[u8]>` impl; see the
/// `Algorithm::RsaPkcs1Sha256` arm above.
fn parse_key<K>(bytes: &[u8]) -> Result<K>
where
    K: for<'a> TryFrom<&'a [u8]>,
{
    // WOLFSSL-API: confirmed — Ed25519 `VerifyingKey`, ECDSA `P256VerifyingKey`/`P384VerifyingKey`
    // all implement `TryFrom<&[u8]>` (fixed-size raw-bytes parse: X9.63 uncompressed point for ECDSA,
    // 32-byte compressed point for Ed25519).
    K::try_from(bytes).map_err(|_| SecurityError::Malformed)
}

/// Parse a signature from raw bytes into a wolfCrypt signature type. Malformed ⇒ fail closed.
fn parse_sig<S>(bytes: &[u8]) -> Result<S>
where
    S: for<'a> TryFrom<&'a [u8]>,
{
    // WOLFSSL-API: confirmed — `Ed25519Sig`, `P256Sig`/`P384Sig` (fixed-size `r‖s`), and
    // `rsa_pkcs1v15::Signature<N>` all implement `TryFrom<&[u8]>` over their fixed-size encoding.
    S::try_from(bytes).map_err(|_| SecurityError::Malformed)
}

/// The single delegation point: verify `sig` over `msg` under `key` via `signature::Verifier`. A
/// non-verifying signature is [`SecurityError::VerificationFailed`] — the ONLY `Ok` is a proven-good
/// signature.
fn verify_generic<K, S>(key: &K, msg: &[u8], sig: &S) -> Result<()>
where
    K: _RcVerifier<S>,
{
    key.verify(msg, sig)
        .map_err(|_| SecurityError::VerificationFailed)
}

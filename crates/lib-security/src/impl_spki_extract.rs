//! The bounded DER `SubjectPublicKeyInfo` extractor.
//!
//! **STUB — not implemented. [`extract_spki`] always fails closed.** ADR 0013 WP-1 owns the real
//! implementation; this file exists so the call site and the contract are named rather than
//! invented later.
//!
//! This is the sharpest edge in the transport-security design: it is the one place that parses
//! attacker-controlled DER, and it runs *before* any trust decision has been made. The contract
//! below is therefore part of the specification, not advice.

use crate::error::{Result, SecurityError};

/// Upper bound, in bytes, on a DER `SubjectPublicKeyInfo` this extractor will emit.
///
/// A P-384 EC `SubjectPublicKeyInfo` is ~120 bytes and an RSA-4096 one ~550; 640 leaves headroom
/// without letting a certificate dictate a buffer size. A `SubjectPublicKeyInfo` longer than this
/// is refused, not truncated.
pub const SPKI_MAX_LEN: usize = 640;

/// Copy the DER `SubjectPublicKeyInfo` out of the DER X.509 certificate in `certificate_der` into
/// `out`, returning its length.
///
/// The extracted bytes are exactly the `SubjectPublicKeyInfo` SEQUENCE — tag, length and contents —
/// so that hashing them yields the SPKI hash a pin is compared against, and so that the result is
/// itself a well-formed DER structure a verifier can consume.
///
/// # Bounded-parse contract
///
/// `certificate_der` is **attacker-controlled** and is parsed before anything about the peer is
/// trusted. A real implementation MUST:
///
/// * **Allocate nothing.** `no_std`, no `alloc`, output into the caller's `out` only.
/// * **Never recurse.** DER is a nested TLV structure and recursion over it is a stack-overflow
///   primitive on a 64 KiB MCU. The walk is an explicit, depth-bounded loop over the fixed,
///   known path into the certificate — a fixed descent, not a general DER parser.
/// * **Run in time bounded by `certificate_der.len()`**, with no backtracking.
/// * **Reject, not repair.** Non-minimal (BER) lengths, indefinite lengths, a length that overruns
///   the input, a truncated header, trailing garbage inside a consumed SEQUENCE, or an unexpected
///   tag are all [`SecurityError::Malformed`]. There is no lenient mode.
/// * **Never read outside `certificate_der`.** Every length is validated against the remaining
///   slice *before* it is used to slice — this crate is `forbid(unsafe_code)`, so the failure mode
///   of a mistake is a panic, and a panic on attacker input is a denial of service. Use checked
///   arithmetic and `get(..)` rather than indexing.
/// * **Enforce [`SPKI_MAX_LEN`]** and fail with [`SecurityError::BufferTooSmall`] when `out` cannot
///   hold the result — never a partial write.
/// * **Not interpret what it extracts.** Curve, algorithm and key validity are somebody else's
///   job; this function locates bytes.
///
/// WP-1 additionally requires a `cargo-fuzz` target and a Miri run over this function before it is
/// trusted with a byte of peer input.
///
/// # Errors
///
/// Always [`SecurityError::NotImplemented`] until WP-1 lands. **Fail closed:** a caller must treat
/// that as "this credential cannot be verified", never as "no SPKI, allow".
///
/// See ADR `docs/decisions/0013-transport-security-trust-store.md` §1 (crate placement) and §7
/// (WP-1).
pub fn extract_spki(certificate_der: &[u8], out: &mut [u8]) -> Result<usize> {
    // Deliberately no parse. Touch the arguments so the signature stays honest under
    // -D warnings without a leading-underscore rename that WP-1 would have to undo.
    let _ = (certificate_der, out);
    Err(SecurityError::NotImplemented)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// The whole point of the stub: it never claims to have found an SPKI.
    #[test]
    fn extract_spki_is_unimplemented_and_fails_closed() {
        let mut out = [0u8; SPKI_MAX_LEN];
        // A syntactically plausible DER SEQUENCE header — a lenient stub might "succeed" here.
        assert_eq!(
            extract_spki(&[0x30, 0x03, 0x02, 0x01, 0x00], &mut out),
            Err(SecurityError::NotImplemented)
        );
        assert_eq!(
            extract_spki(&[], &mut out),
            Err(SecurityError::NotImplemented)
        );
    }
}

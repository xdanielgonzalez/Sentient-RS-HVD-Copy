use super::prelude::*;

use super::{decode_hex, SIGNATURE_HEX_LEN};

impl TokenVerifier for SoftwareTokenVerifier {
    fn verify_token(
        &self,
        token: &[u8],
        trusted_key: &[u8],
        policy: &TokenPolicy<'_>,
    ) -> Result<TokenClaims> {
        if token.len() <= SIGNATURE_HEX_LEN {
            return Err(SecurityError::Malformed);
        }
        let (claims_bytes, sig_hex) = token.split_at(token.len() - SIGNATURE_HEX_LEN);

        let mut sig_bytes = [0u8; SIGNATURE_LEN];
        decode_hex(sig_hex, &mut sig_bytes)?;

        // Signature FIRST — cryptographic proof before any claims content is trusted.
        verify_ecdsa_p256(trusted_key, claims_bytes, &sig_bytes)?;

        let claims_text =
            core::str::from_utf8(claims_bytes).map_err(|_| SecurityError::Malformed)?;
        let mut parts = claims_text.split('|');
        let (Some(subject), Some(issuer), Some(audience), Some(expires_at), None) = (
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
        ) else {
            return Err(SecurityError::Malformed);
        };
        let expires_at: u64 = expires_at.parse().map_err(|_| SecurityError::Malformed)?;

        if issuer != policy.issuer || audience != policy.audience {
            return Err(SecurityError::IdentityMismatch);
        }
        if expires_at <= policy.now_unix {
            return Err(SecurityError::Expired);
        }

        let mut claims = TokenClaims {
            expires_at,
            ..Default::default()
        };
        claims
            .subject
            .push_str(subject)
            .map_err(|_| SecurityError::BufferTooSmall)?;
        claims
            .issuer
            .push_str(issuer)
            .map_err(|_| SecurityError::BufferTooSmall)?;
        claims
            .audience
            .push_str(audience)
            .map_err(|_| SecurityError::BufferTooSmall)?;
        Ok(claims)
    }
}

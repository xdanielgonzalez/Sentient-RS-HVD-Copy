use super::prelude::*;

impl TokenVerifier for FakeTokenVerifier {
    fn verify_token(
        &self,
        token: &[u8],
        trusted_key: &[u8],
        policy: &TokenPolicy<'_>,
    ) -> lib_security::Result<TokenClaims> {
        let token = core::str::from_utf8(token).map_err(|_| SecurityError::Malformed)?;
        let mut parts = token.splitn(5, '|');
        let (Some(subject), Some(issuer), Some(audience), Some(expires_at), Some(secret)) = (
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
        ) else {
            return Err(SecurityError::Malformed);
        };
        let expires_at: u64 = expires_at.parse().map_err(|_| SecurityError::Malformed)?;

        if secret.as_bytes() != trusted_key {
            return Err(SecurityError::VerificationFailed);
        }
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

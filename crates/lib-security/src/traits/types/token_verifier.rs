use super::token_claims::TokenClaims;
use super::token_policy::TokenPolicy;
use crate::error::Result;

/// Auth-token verification (e.g. JWT). Fail closed: returns the verified claims only if the
/// signature, expiry, issuer, and audience all check.
pub trait TokenVerifier {
    /// Verify `token` against `policy` using `trusted_key` (the issuer's public signing key),
    /// returning the claims iff every check passes.
    fn verify_token(
        &self,
        token: &[u8],
        trusted_key: &[u8],
        policy: &TokenPolicy<'_>,
    ) -> Result<TokenClaims>;
}

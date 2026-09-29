/// Verified claims extracted from an auth token. Bounded, no allocation: subject/issuer/audience are
/// copied into fixed-capacity buffers.
#[derive(Debug, Clone, Default)]
pub struct TokenClaims {
    /// Subject (`sub`).
    pub subject: heapless::String<64>,
    /// Issuer (`iss`).
    pub issuer: heapless::String<64>,
    /// Audience (`aud`).
    pub audience: heapless::String<64>,
    /// Expiry (`exp`), seconds since the Unix epoch.
    pub expires_at: u64,
}

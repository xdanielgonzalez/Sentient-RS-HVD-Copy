/// Requirements a token must satisfy to be accepted.
#[derive(Debug, Clone, Copy)]
pub struct TokenPolicy<'a> {
    /// Required issuer.
    pub issuer: &'a str,
    /// Required audience.
    pub audience: &'a str,
    /// Current time (seconds since the Unix epoch) for the expiry check — injected, never read from
    /// a global clock, so verification is testable and deterministic.
    pub now_unix: u64,
}

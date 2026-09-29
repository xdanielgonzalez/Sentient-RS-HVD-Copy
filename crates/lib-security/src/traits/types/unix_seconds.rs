/// A wall-clock instant, seconds since the Unix epoch (1970-01-01T00:00:00Z), UTC, no leap-second
/// adjustment. The unit the credential-validity vocabulary of X.509 (`notBefore`/`notAfter`) speaks.
///
/// This type carries **no trust claim**. Whether the value may be used for a security decision is
/// reported separately by [`super::trusted_time::TrustedTime::trust`] — a `UnixSeconds` obtained
/// from an unauthenticated source (SNTP, a peer's assertion) is an availability input only. Keeping
/// the value and its trustworthiness in two places is deliberate: a bare integer cannot smuggle a
/// trust claim past a caller.
///
/// `u64` rather than `i64`: pre-epoch instants are not representable, which is correct here — no
/// credential this device handles is dated before 1970, and the unsigned type removes a whole class
/// of sign-confusion arithmetic from the validity comparisons a future `X509Chain` trust model
/// would perform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct UnixSeconds(pub u64);

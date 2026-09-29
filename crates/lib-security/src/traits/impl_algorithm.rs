use super::prelude::*;

impl Algorithm {
    /// Whether this algorithm is in the FIPS 140-3 approved set. Pure classification (no crypto) —
    /// the single source of truth behind [`crate::Provider::check_approved`]. Ed25519 is the one
    /// listed algorithm that is not approved.
    ///
    /// Ed25519's `NotApproved` status is a **module-certificate** gate, not a standards gate:
    /// FIPS 186-5 (Feb 2023) specifies Ed25519 outright, but our validated crypto module —
    /// wolfCrypt — does not list it on either of its FIPS 140-3 certificates (#4718, #5041) for
    /// our operating environment, and CNSA 2.0 excludes it entirely (mandating ML-DSA-87 in its
    /// place). Do not flip this to `true` without a validated module certificate that lists
    /// Ed25519 for our operating environment — doing so under an NSS designation would be a
    /// CNSA violation. See Gate-0 tracking issue #351 before touching this.
    pub const fn is_fips_approved(&self) -> bool {
        !matches!(self, Algorithm::Ed25519)
    }
}

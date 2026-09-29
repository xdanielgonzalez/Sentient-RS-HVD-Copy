//! The active security tier, derived from compile-time features and reported at runtime.
//!
//! The toggles compose into a posture that is **honest about custody and validation**: turning a
//! toggle off is never silently equivalent to turning it on, and — because the identity key and
//! session keys can live at *different* custody tiers on the same device (identity in a secure
//! element, sessions in software, say) — [`SecurityProfile`] tracks custody **per
//! [`KeyPurpose`]** rather than collapsing them into one field that could overstate the weaker
//! key's protection. Callers (and logs, and the compliance report) can read a specific purpose's
//! custody via [`SecurityProfile::custody`], or the worst-case summary via
//! [`SecurityProfile::weakest_custody`] — the summary a single global field used to silently be.

mod impl_key_custody;
mod impl_security_profile;
mod prelude;
mod types;

pub use types::*;

#[cfg(test)]
mod tests {
    use super::*;

    /// The one build-specific check: `SecurityProfile::active()` reflects the compile-time feature
    /// toggles. The behavioral grid (tiers × algorithms × errors) lives in `tests/matrix.rs`
    /// (rstest), which supersedes the old hand-written unit tests.
    #[test]
    fn active_profile_matches_features() {
        let p = SecurityProfile::active();
        assert_eq!(p.fips_enabled(), cfg!(feature = "fips"));
        assert_eq!(p.secure_world_enabled(), cfg!(feature = "trustzone"));
        assert!(!p.tier_slug().is_empty());
    }
}

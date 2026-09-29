use super::crypto_validation::CryptoValidation;
use super::key_custody::KeyCustody;

/// The runtime-readable security posture, fixed at build time by the feature toggles.
///
/// Custody is tracked **per purpose** ([`identity_custody`](SecurityProfile::identity_custody) /
/// [`session_custody`](SecurityProfile::session_custody)) rather than one global tier: the
/// identity key can sit in a secure element while session keys are established in software (or in
/// TrustZone-S but not a secure element), and a single field would misreport whichever key is
/// actually weaker. Use [`SecurityProfile::custody`] to read one purpose's tier, or
/// [`SecurityProfile::weakest_custody`] for the honest worst-case summary that
/// [`SecurityProfile::tier_slug`] and [`SecurityProfile::custody_warning`] are built on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SecurityProfile {
    /// Custody of the identity key (the long-lived device-identity signing key).
    pub identity_custody: KeyCustody,
    /// Custody of session keys (established per connection/session).
    pub session_custody: KeyCustody,
    /// Cryptographic-module validation status.
    pub validation: CryptoValidation,
}

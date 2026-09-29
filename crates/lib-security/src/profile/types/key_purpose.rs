/// Which key a [`super::security_profile::SecurityProfile`] custody field covers.
///
/// The profile tracks custody **per purpose** rather than one global tier, because the two keys
/// can legitimately live at different custody tiers on the same device: the identity key may sit
/// in a discrete secure element while session keys are established in software (or in TrustZone-S
/// but not a secure element). Collapsing that into one field would misreport whichever key is
/// actually weaker — see [`super::security_profile::SecurityProfile::weakest_custody`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyPurpose {
    /// The device identity key — the long-lived key that proves *which device this is* (e.g. the
    /// mTLS/TLS client-certificate signing key). Provisioned once, held for the device's lifetime.
    Identity,
    /// Session (ephemeral/negotiated) keys — established per connection/session and used for
    /// bulk authenticated encryption (see [`crate::SessionCipher`]), not for proving identity.
    Session,
}

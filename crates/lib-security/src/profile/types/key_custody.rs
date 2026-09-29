/// Custody tier of a private key: where it physically lives and what can extract it.
///
/// Strength order, strongest to weakest: `SecureElement > SecureWorld > SoftwareMemory`
/// ([`KeyCustody::SecureElement`], [`KeyCustody::SecureWorld`], [`KeyCustody::SoftwareMemory`]).
/// See `KeyCustody::weaker_of` (`impl_key_custody.rs`) for the const-fn ordering
/// [`super::security_profile::SecurityProfile::weakest_custody`] uses to combine the identity and
/// session custody tiers into one honest summary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCustody {
    /// Keys live in a discrete secure element (e.g. an SE05x, discrete TPM) behind its own
    /// hardware boundary, referenced by opaque handle only. Strongest tier: not exportable even to
    /// a compromised secure-world (TrustZone-S) OS, since the secure element is a separate chip.
    /// (`feature = "secure-element"`.)
    SecureElement,
    /// Keys live in the secure world (ARM TrustZone-S / TF-M) or another on-die hardware isolation
    /// domain, referenced by opaque handle only. (`feature = "trustzone"`.)
    SecureWorld,
    /// Keys live in normal-world memory (zeroized on drop). The lowest assurance tier.
    SoftwareMemory,
}

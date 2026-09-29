use super::prelude::*;

impl SecurityProfile {
    /// The active profile, resolved from compile-time features.
    ///
    /// * `secure-element` ⇒ **identity** custody is [`KeyCustody::SecureElement`]; else
    ///   `trustzone` ⇒ [`KeyCustody::SecureWorld`]; else [`KeyCustody::SoftwareMemory`].
    /// * `trustzone` ⇒ **session** custody is [`KeyCustody::SecureWorld`]; else
    ///   [`KeyCustody::SoftwareMemory`]. `secure-element` alone does NOT raise session custody — a
    ///   secure element in this crate's model custodies the identity key only; session keys still
    ///   need `trustzone` (or a future session-capable secure-element path) to move off software.
    /// * `fips` ⇒ [`CryptoValidation::Fips140_3`]; else [`CryptoValidation::Standard`].
    pub const fn active() -> Self {
        let identity_custody = if cfg!(feature = "secure-element") {
            KeyCustody::SecureElement
        } else if cfg!(feature = "trustzone") {
            KeyCustody::SecureWorld
        } else {
            KeyCustody::SoftwareMemory
        };
        let session_custody = if cfg!(feature = "trustzone") {
            KeyCustody::SecureWorld
        } else {
            KeyCustody::SoftwareMemory
        };
        SecurityProfile {
            identity_custody,
            session_custody,
            validation: if cfg!(feature = "fips") {
                CryptoValidation::Fips140_3
            } else {
                CryptoValidation::Standard
            },
        }
    }

    /// True if cryptography runs inside a FIPS-validated module.
    pub const fn fips_enabled(&self) -> bool {
        matches!(self.validation, CryptoValidation::Fips140_3)
    }

    /// Custody of the key covering `purpose` (identity or session — see [`KeyPurpose`]).
    pub const fn custody(&self, purpose: KeyPurpose) -> KeyCustody {
        match purpose {
            KeyPurpose::Identity => self.identity_custody,
            KeyPurpose::Session => self.session_custody,
        }
    }

    /// The weaker of the identity and session custody tiers — the honest, worst-case summary
    /// ([`KeyCustody::weaker_of`]). Mixed custody (e.g. identity in a secure element, session in
    /// software) reports as the weaker of the two: a summary must never overstate protection that
    /// only one of the two keys actually has.
    pub const fn weakest_custody(&self) -> KeyCustody {
        KeyCustody::weaker_of(self.identity_custody, self.session_custody)
    }

    /// True iff **both** the identity and session keys are hardware-backed — i.e.
    /// [`weakest_custody`](Self::weakest_custody) is never [`KeyCustody::SoftwareMemory`].
    ///
    /// Kept named `secure_world_enabled` for API/log continuity with the pre-per-purpose profile;
    /// its documented meaning is now honest under mixed custody: it does NOT mean "the session key
    /// is in TrustZone" specifically (that's [`Self::custody`]`(`[`KeyPurpose::Session`]`)`), it
    /// means "no purpose fell back to software" — `secure-element`-only (identity hardware,
    /// session software) is correctly `false` here, matching [`Self::custody_warning`] firing.
    pub const fn secure_world_enabled(&self) -> bool {
        !matches!(self.weakest_custody(), KeyCustody::SoftwareMemory)
    }

    /// A startup warning naming whichever purpose(s) fell back to software custody, `None` when
    /// neither did. Emit this at boot so an operator can never believe a software-held key is
    /// hardware-bound (the false-security trap):
    /// `if let Some(w) = profile.custody_warning() { log::warn!("{w}") }`.
    ///
    /// Const-friendly: matches on the `(identity, session)` custody pair directly (no
    /// `weakest_custody()` call needed) so it can name which purpose(s) are weak, not just that one
    /// is.
    pub const fn custody_warning(&self) -> Option<&'static str> {
        use KeyCustody::SoftwareMemory as Sw;
        match (self.identity_custody, self.session_custody) {
            (Sw, Sw) => Some(
                "key custody = SoftwareMemory for BOTH the identity and session keys (NOT \
                 hardware-backed; keys are exfiltrable by a compromised host)",
            ),
            (Sw, _) => Some(
                "identity key custody = SoftwareMemory (NOT hardware-backed; the identity key is \
                 exfiltrable by a compromised host)",
            ),
            (_, Sw) => Some(
                "session key custody = SoftwareMemory (NOT hardware-backed; session keys are \
                 exfiltrable by a compromised host)",
            ),
            _ => None,
        }
    }

    /// A stable slug for logs / the compliance report, e.g. `fips+trustzone`, `standard+software`,
    /// built from `(validation, weakest_custody())`.
    ///
    /// **Weakest-wins, so the slug set stays exactly the four legacy spellings** —
    /// `standard+software`, `standard+trustzone`, `fips+software`, `fips+trustzone` —
    /// [`KeyCustody::SecureElement`] never appears in a slug. Reasoning: `active()` only ever
    /// raises **identity** custody to `SecureElement`; session custody tops out at `SecureWorld`.
    /// So `secure-element` alone (identity = `SecureElement`, session = `SoftwareMemory`) has
    /// `weakest_custody() == SoftwareMemory` ⇒ `"standard+software"` — the slug correctly does NOT
    /// advertise hardware custody the session key doesn't have. `secure-element` + `trustzone`
    /// (identity = `SecureElement`, session = `SecureWorld`) has `weakest_custody() ==
    /// SecureWorld` ⇒ `"standard+trustzone"`, same slug as plain `trustzone`: the slug distinguishes
    /// only *whether every purpose is hardware-backed*, not *which* hardware tier — that finer
    /// detail is what [`Self::custody`] is for. (If a future build ever constructs a profile with
    /// session custody also at `SecureElement`, `weakest_custody()` still resolves to
    /// `SecureElement` here and this reports the same as `SecureWorld` — a hardware-backed tier is
    /// a hardware-backed tier as far as this slug's `fips`/`software` × `trustzone` axes go.)
    pub const fn tier_slug(&self) -> &'static str {
        match (self.validation, self.weakest_custody()) {
            (CryptoValidation::Fips140_3, KeyCustody::SecureElement | KeyCustody::SecureWorld) => {
                "fips+trustzone"
            }
            (CryptoValidation::Fips140_3, KeyCustody::SoftwareMemory) => "fips+software",
            (CryptoValidation::Standard, KeyCustody::SecureElement | KeyCustody::SecureWorld) => {
                "standard+trustzone"
            }
            (CryptoValidation::Standard, KeyCustody::SoftwareMemory) => "standard+software",
        }
    }
}

impl Default for SecurityProfile {
    fn default() -> Self {
        SecurityProfile::active()
    }
}

use super::prelude::*;

impl KeyCustody {
    /// The weaker (lower-assurance) of two custody tiers, by the strength order
    /// `SecureElement > SecureWorld > SoftwareMemory`.
    ///
    /// Hand-written instead of `derive(Ord)` + `Ord::min`: trait methods like `Ord::cmp`/`min` are
    /// not callable from a `const fn` on stable Rust, and this function must stay `const` because
    /// its callers — [`SecurityProfile::weakest_custody`](super::SecurityProfile::weakest_custody),
    /// [`SecurityProfile::secure_world_enabled`](super::SecurityProfile::secure_world_enabled), and
    /// [`SecurityProfile::tier_slug`](super::SecurityProfile::tier_slug) — are themselves `const
    /// fn`s. A plain hand-written `match` has no such restriction.
    pub const fn weaker_of(a: KeyCustody, b: KeyCustody) -> KeyCustody {
        use KeyCustody::{SecureElement, SecureWorld, SoftwareMemory};
        match (a, b) {
            (SoftwareMemory, _) | (_, SoftwareMemory) => SoftwareMemory,
            (SecureWorld, _) | (_, SecureWorld) => SecureWorld,
            (SecureElement, SecureElement) => SecureElement,
        }
    }
}

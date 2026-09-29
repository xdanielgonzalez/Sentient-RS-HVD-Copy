use super::prelude::*;
use super::{approved_under_fips, run_power_on_self_tests};

impl Provider {
    /// Construct the provider for the active tier. Under FIPS this runs the module's power-on
    /// self-tests (Known-Answer-Tests); a failure yields [`SecurityError::SelfTestFailed`] and the
    /// provider must not be used.
    pub fn new() -> Result<Self> {
        let profile = SecurityProfile::active();
        if profile.fips_enabled() {
            run_power_on_self_tests()?;
        }
        Ok(Provider { profile })
    }

    /// The active security posture.
    pub fn profile(&self) -> SecurityProfile {
        self.profile
    }

    /// The approved-algorithm gate. Under FIPS, refuses any algorithm outside the FIPS 140-3
    /// approved set (fail closed with [`SecurityError::NotApproved`]); outside FIPS, all listed
    /// algorithms are permitted. This is the boundary rule that prevents a non-approved primitive
    /// from being used in a validated build — enforced independent of which backend is bound.
    pub fn check_approved(&self, algorithm: Algorithm) -> Result<()> {
        approved_under_fips(algorithm, self.profile.fips_enabled())
    }
}

//! Provider selection and the approved-algorithm gate.
//!
//! The concrete cryptographic backends (software, FIPS-validated module, secure-world / TrustZone)
//! are wired per `docs/security-integration-map.md` through the enforced planner→scaffold→fill→gate
//! pipeline. This module provides the parts that are pure policy — and therefore correct and
//! testable *now*, before any crypto is bound:
//!
//! * [`Provider::profile`] — the active [`SecurityProfile`].
//! * [`Provider::check_approved`] — the FIPS approved-algorithm gate (refuses non-approved
//!   primitives when `feature = "fips"`), which is a cryptographic-boundary rule, not crypto math.
//! * Construction runs the module self-test hook (KATs) when FIPS is active.
//!
//! The crypto operations themselves (`Rng`, `Hasher`, `KeyStore`, `Verifier`, …) are **fail-closed
//! stubs** here: they return [`SecurityError::NotImplemented`] rather than a fake success, so the
//! seam compiles and can be integrated without any code ever mistaking an unbound backend for a
//! working one.
//!
//! ## Concrete backend selection (`feature = "wolfcrypt"`)
//! `feature = "wolfcrypt"` supplies the concrete backend
//! (`crate::backends::wolfcrypt` — `WolfRng`/`WolfHasher`/`WolfVerifier`/`WolfKeyStore`) and
//! **composes with** this module's policy rather than replacing it: the `fips` toggle selects
//! wolfCrypt FIPS mode (the FIPS 140-3 validated wolfCrypt module), while
//! [`Provider::check_approved`] and `run_power_on_self_tests` remain the approved-algorithm gate
//! and KAT hook that policy stays responsible for. `Provider` does not yet route to the backend —
//! enabling `wolfcrypt` does not
//! change the fail-closed default behavior here; it only makes the concrete implementations
//! available. wolfCrypt requires a system wolfSSL 5.9.0+ library at link time (a separate build
//! ledger, like the Zephyr `west` target) and does not link in the default build.

mod impl_provider;
mod prelude;
mod types;

use prelude::*;

pub use types::*;

/// The approved-algorithm gate as a pure function of `(algorithm, fips_enabled)` — the testable
/// core of [`Provider::check_approved`]. With FIPS off, every listed algorithm is permitted; with
/// FIPS on, only the approved set (see [`Algorithm::is_fips_approved`]) passes, the rest fail closed
/// with [`SecurityError::NotApproved`].
pub fn approved_under_fips(algorithm: Algorithm, fips_enabled: bool) -> Result<()> {
    if !fips_enabled || algorithm.is_fips_approved() {
        Ok(())
    } else {
        Err(SecurityError::NotApproved)
    }
}

/// FIPS power-on self-tests (KATs over each approved algorithm). Bound to the real validated module
/// via the pipeline; until then it fails closed so a FIPS build cannot pretend it self-tested.
fn run_power_on_self_tests() -> Result<()> {
    // Concrete KATs are wired with the FIPS backend (security-integration-map §FIPS). A FIPS build
    // must not run unverified: fail closed until the real self-tests are bound.
    Err(SecurityError::NotImplemented)
}

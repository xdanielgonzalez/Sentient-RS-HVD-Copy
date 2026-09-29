use super::prelude::*;

// Unconditional (not `#[cfg(feature = "std")]`): `core::error::Error` is `no_std`-available (stable
// since Rust 1.81) and IS `std::error::Error` — `std` re-exports the same trait rather than defining
// a second one — so this single impl satisfies both an `std::error::Error` bound under `feature =
// "std"` and a `core::error::Error` bound in the `no_std` core (needed by `rand_core::TryRng::Error:
// core::error::Error` for the software backend's RNG adapter — see `backends::software`).
impl core::error::Error for SecurityError {}

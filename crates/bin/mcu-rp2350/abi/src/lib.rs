//! `rp2350-abi` — the frozen NS<->S ABI for the RP2350 TrustZone-M demo split (EPIC #350, story #355).
//! Demo scaffold, not production: claims limited to signed boot / load-time integrity / IP protection
//! vs non-physical attackers; no FIPS, no attestation, no anti-tamper; RP2350 A4 stepping only.
//!
//! The four `cmse_nonsecure_entry` veneers in `../veneers/veneer.c` are the only way non-secure code
//! reaches the secure world; each validates NS pointers via `cmse_check_address_range`, then
//! copy-in/copy-outs through an S-side scratch buffer capped at [`SV_MAX_PAYLOAD`] bytes.
//! [`types::SvError`] and [`SV_MAX_PAYLOAD`] are the source of truth — `veneer.h`'s mirrored
//! `#define`s must be kept in sync by hand (no cbindgen step here). `no_std`, target-independent:
//! no C FFI or cortex-m/embassy deps, so it builds and its `#[cfg(test)]` tests run on a host
//! target with no emulator.
#![no_std]
#![forbid(unsafe_code)]

pub mod types;

mod impl_sv_custody;
mod impl_sv_error;

pub use impl_sv_custody::{custody_code_to_key_custody, key_custody_to_custody_code};
pub use impl_sv_error::{security_error_to_sv_err, sv_err_to_security_error};
pub use types::{SvCustody, SvError};

/// Max payload `sv_seal`/`sv_open` accept per call, in bytes (frozen ABI constant — mirrored in
/// `veneer.h`). A larger payload is refused with [`types::SvError::Capacity`]
/// (`SV_ERR_CAPACITY = -5`); chunking is TODO(#355), documented, not implemented in this scaffold.
pub const SV_MAX_PAYLOAD: u32 = 4096;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use lib_security::{KeyCustody, SecurityError};

    // --- const-asserts on the frozen numeric codes themselves (catches an accidental edit to the
    // ABI at compile time, on the host, before it ever reaches a build of `s` or `veneer.c`) ---
    const _: () = assert!(SvError::NotImplemented.code() == -1);
    const _: () = assert!(SvError::KeyNotFound.code() == -2);
    const _: () = assert!(SvError::VerificationFailed.code() == -3);
    const _: () = assert!(SvError::BufferTooSmall.code() == -4);
    const _: () = assert!(SvError::Capacity.code() == -5);
    const _: () = assert!(SvError::BadPointer.code() == -6);
    const _: () = assert!(SvError::Backend.code() == -7);

    const _: () = assert!(SvCustody::SoftwareMemory.code() == 0);
    const _: () = assert!(SvCustody::SecureWorld.code() == 1);
    const _: () = assert!(SvCustody::SecureElement.code() == 2);

    const _: () = assert!(SV_MAX_PAYLOAD == 4096);

    #[test]
    fn sv_err_round_trips_through_security_error_where_lossless() {
        // The four codes with a 1:1 SecurityError counterpart must round-trip exactly.
        assert_eq!(
            sv_err_to_security_error(SvError::NotImplemented.code()),
            SecurityError::NotImplemented
        );
        assert_eq!(
            sv_err_to_security_error(SvError::KeyNotFound.code()),
            SecurityError::KeyNotFound
        );
        assert_eq!(
            sv_err_to_security_error(SvError::VerificationFailed.code()),
            SecurityError::VerificationFailed
        );
        assert_eq!(
            sv_err_to_security_error(SvError::BufferTooSmall.code()),
            SecurityError::BufferTooSmall
        );
        assert_eq!(
            security_error_to_sv_err(SecurityError::NotImplemented),
            SvError::NotImplemented.code()
        );
        assert_eq!(
            security_error_to_sv_err(SecurityError::KeyNotFound),
            SvError::KeyNotFound.code()
        );
        assert_eq!(
            security_error_to_sv_err(SecurityError::VerificationFailed),
            SvError::VerificationFailed.code()
        );
        assert_eq!(
            security_error_to_sv_err(SecurityError::BufferTooSmall),
            SvError::BufferTooSmall.code()
        );
    }

    #[test]
    fn sv_err_lossy_mappings_documented_here_stay_stable() {
        // CAPACITY folds to BufferTooSmall (closest existing SecurityError variant — see
        // impl_sv_error.rs's doc for why there is no dedicated "internal scratch capacity"
        // SecurityError variant).
        assert_eq!(
            sv_err_to_security_error(SvError::Capacity.code()),
            SecurityError::BufferTooSmall
        );
        // BAD_POINTER folds to Malformed (closest existing variant — an invalid FFI pointer
        // describes malformed input at the boundary; lib-security's core traits never see raw
        // pointers, only slices, so there is no dedicated "bad pointer" variant).
        assert_eq!(
            sv_err_to_security_error(SvError::BadPointer.code()),
            SecurityError::Malformed
        );
        assert_eq!(
            sv_err_to_security_error(SvError::Backend.code()),
            SecurityError::Backend
        );
    }

    #[test]
    fn unreachable_security_error_variants_fold_to_backend() {
        // These SecurityError variants describe FIPS/token/cert-chain conditions this session-only
        // veneer surface never produces (no TokenVerifier/CertVerifier/FIPS self-test call is
        // reachable from sv_seal/sv_open/sv_session_establish/sv_profile) — documented fallback is
        // SV_ERR_BACKEND, never a fabricated success.
        for err in [
            SecurityError::NotApproved,
            SecurityError::SelfTestFailed,
            SecurityError::EntropyFailure,
            SecurityError::KeyExportForbidden,
            SecurityError::Expired,
            SecurityError::UntrustedChain,
            SecurityError::IdentityMismatch,
        ] {
            assert_eq!(security_error_to_sv_err(err), SvError::Backend.code());
        }
    }

    #[test]
    fn custody_codes_round_trip() {
        assert_eq!(
            key_custody_to_custody_code(KeyCustody::SoftwareMemory),
            SvCustody::SoftwareMemory.code()
        );
        assert_eq!(
            key_custody_to_custody_code(KeyCustody::SecureWorld),
            SvCustody::SecureWorld.code()
        );
        assert_eq!(
            key_custody_to_custody_code(KeyCustody::SecureElement),
            SvCustody::SecureElement.code()
        );
    }

    #[test]
    fn unknown_code_from_veneer_falls_back_to_backend() {
        // Any i32 outside the 7 frozen codes is a programming error at the FFI boundary, not a
        // reason to panic or guess — fail closed to SecurityError::Backend.
        assert_eq!(sv_err_to_security_error(0), SecurityError::Backend);
        assert_eq!(sv_err_to_security_error(-99), SecurityError::Backend);
        assert_eq!(sv_err_to_security_error(1), SecurityError::Backend);
    }
}

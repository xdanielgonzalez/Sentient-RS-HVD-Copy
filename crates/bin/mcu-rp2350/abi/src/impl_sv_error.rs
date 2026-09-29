use lib_security::SecurityError;

use crate::types::SvError;

impl SvError {
    /// This variant's frozen ABI code (the `i32` a veneer actually returns).
    pub const fn code(self) -> i32 {
        self as i32
    }
}

/// Map a raw veneer return code to the [`SecurityError`] a caller should treat it as. Intended for
/// (eventual) NS-side use: a caller that gets a negative `i32` back from an `sv_*` veneer call and
/// wants this crate's shared vocabulary for it, rather than re-deriving the mapping.
///
/// Four codes have an exact 1:1 [`SecurityError`] counterpart: [`SvError::NotImplemented`],
/// [`SvError::KeyNotFound`], [`SvError::VerificationFailed`], [`SvError::BufferTooSmall`].
///
/// The remaining three are FFI/scaffold-boundary conditions with no dedicated `SecurityError`
/// variant, mapped to the closest existing one (documented, not silently equivalent):
/// * [`SvError::Capacity`] (`len`/`aad_len` exceeds this build's fixed scratch) →
///   [`SecurityError::BufferTooSmall`] — both describe "refused because a size bound was exceeded";
///   `lib-security`'s core traits have no notion of a *veneer-internal* scratch limit distinct from
///   the *caller's output buffer* limit, so the two fold together on the Rust side.
/// * [`SvError::BadPointer`] (a NS pointer failed `cmse_check_address_range`) →
///   [`SecurityError::Malformed`] — `lib-security`'s traits operate on slices, never raw pointers,
///   so there is no "bad pointer" variant; an invalid pointer at the FFI boundary is, from the
///   library's perspective, malformed input.
/// * [`SvError::Backend`] → [`SecurityError::Backend`] (exact match).
///
/// Any `code` outside the seven frozen values is a programming error at the FFI boundary, not a
/// reason to panic or guess — fails closed to [`SecurityError::Backend`].
pub const fn sv_err_to_security_error(code: i32) -> SecurityError {
    match code {
        c if c == SvError::NotImplemented.code() => SecurityError::NotImplemented,
        c if c == SvError::KeyNotFound.code() => SecurityError::KeyNotFound,
        c if c == SvError::VerificationFailed.code() => SecurityError::VerificationFailed,
        c if c == SvError::BufferTooSmall.code() => SecurityError::BufferTooSmall,
        c if c == SvError::Capacity.code() => SecurityError::BufferTooSmall,
        c if c == SvError::BadPointer.code() => SecurityError::Malformed,
        c if c == SvError::Backend.code() => SecurityError::Backend,
        _ => SecurityError::Backend,
    }
}

/// The inverse direction: map a [`SecurityError`] the S-side service produces to the frozen `i32`
/// code a veneer returns. Not named in the frozen ABI spec itself (that only requires
/// `sv_err_to_security_error`), but required for the `s` crate (this ABI's only producer of these
/// codes) to actually emit them — kept here, alongside the forward mapping, as the ABI's single
/// source of truth for both directions.
///
/// The four variants with an exact code carry it straight through. The rest are variants this
/// session-only veneer surface (`sv_session_establish`/`sv_seal`/`sv_open`/`sv_profile`) can never
/// actually produce in this scaffold — no `TokenVerifier`/`CertVerifier`/FIPS self-test call is
/// reachable from a `SessionCipher` operation — so they fold to [`SvError::Backend`] rather than
/// inventing a code for a condition that cannot occur:
/// * [`SecurityError::Malformed`] → [`SvError::BadPointer`] (mirrors the forward mapping's choice,
///   even though `Malformed` is reachable here too — see `SoftwareSessionStore::open`'s "`len` too
///   short to hold the framing" case — kept `BadPointer` for symmetry with the forward direction;
///   this is the one genuinely lossy fold: the *cause* (a badly-framed `sv_open` input) is real, the
///   ABI code chosen for it is a re-use of a code named for a different cause).
/// * [`SecurityError::NotApproved`], [`SecurityError::SelfTestFailed`],
///   [`SecurityError::EntropyFailure`], [`SecurityError::KeyExportForbidden`],
///   [`SecurityError::Expired`], [`SecurityError::UntrustedChain`],
///   [`SecurityError::IdentityMismatch`] → [`SvError::Backend`] (unreachable from this ABI surface
///   in this scaffold; documented rather than silently coerced to a misleading code).
pub const fn security_error_to_sv_err(err: SecurityError) -> i32 {
    match err {
        SecurityError::NotImplemented => SvError::NotImplemented.code(),
        SecurityError::KeyNotFound => SvError::KeyNotFound.code(),
        SecurityError::VerificationFailed => SvError::VerificationFailed.code(),
        SecurityError::BufferTooSmall => SvError::BufferTooSmall.code(),
        SecurityError::Malformed => SvError::BadPointer.code(),
        _ => SvError::Backend.code(),
    }
}

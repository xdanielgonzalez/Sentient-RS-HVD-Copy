/// Frozen `sv_*` veneer error codes (negative `i32`, `story #355` ABI). Every veneer entry point
/// returns `>= 0` on success or one of these as a negative `i32` — never a fabricated success.
///
/// These discriminants ARE the ABI: they are mirrored verbatim (by hand, not codegen — see the
/// crate doc) in `../../veneers/veneer.h`'s `#define SV_ERR_*` constants. Changing a discriminant
/// here without updating the header breaks the frozen contract.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SvError {
    /// The requested operation is not implemented by this build's backend. Fail closed.
    NotImplemented = -1,
    /// `handle` does not reference a live session in the S-side store.
    KeyNotFound = -2,
    /// `sv_open`'s authenticated decryption did not verify (tampered ciphertext/tag, or wrong key).
    VerificationFailed = -3,
    /// The caller-supplied `buf`/`cap` cannot hold the operation's output (e.g. `cap` too small for
    /// `sv_seal`'s framed ciphertext, or `len` too short to hold `sv_open`'s expected framing).
    BufferTooSmall = -4,
    /// `len` (or `aad_len`) exceeds this scaffold's fixed internal scratch capacity —
    /// [`crate::SV_MAX_PAYLOAD`] for the payload, or the veneer's own fixed AAD scratch size (see
    /// `veneer.h`). Chunking a larger payload across multiple calls is TODO(#355).
    Capacity = -5,
    /// A pointer argument (`buf`, `aad`, or an out-param) failed `cmse_check_address_range`
    /// validation against the non-secure world (NULL, or the range is not fully NS-accessible).
    BadPointer = -6,
    /// The underlying S-side crypto/service backend reported a failure this ABI has no more
    /// specific code for (see `impl_sv_error.rs`'s doc for exactly which `SecurityError` variants
    /// fold here).
    Backend = -7,
}

//! `SessionCipher` impl for [`VeneerSessionCipher`], riding the frozen S/NS veneer ABI
//! (`sv_session_establish`/`sv_seal`/`sv_open`/`sv_profile`) plus the raw `extern "C"` bindings.
//! Veneers (`-mcmse`, `cmse_nonsecure_entry`) live in Agent A's `s`/`veneers` crates; NS links via
//! `ns/build.rs`'s `SENTIENT_VENEER_IMPLIB` step.
//!
//! `sv_seal`/`sv_open` bundle their four trailing arguments into one NS-pointer struct
//! (`sv_crypto_args_t` / [`SvCryptoArgs`], field-for-field, `repr(C)`) because
//! `cmse_nonsecure_entry` caps entry-point arguments at 4 AAPCS registers — the original
//! 6-plain-argument signature cannot exist as a CMSE entry.
//!
//! **Demo scaffold**: signed-boot / load-time-integrity / IP-protection posture only, RP2350 A4
//! stepping. NO FIPS/attestation/anti-tamper claim.
//!
//! **Fail-closed `SV_MAX_PAYLOAD`.** The S-side veneer copies into a fixed 4096-byte static
//! scratch with no chunking (TODO(#355)); [`seal_or_open`] checks `cap`/`len` fit BEFORE calling,
//! failing closed with [`SecurityError::BufferTooSmall`] rather than truncating. Custody-code
//! mapping (`sv_profile`) goes through `rp2350_abi::custody_code_to_key_custody`, shared with the
//! `s` crate that emits these codes.

use core::cell::Cell;

use crate::prelude::*;

/// Mirrors `../../veneers/veneer.h`'s `sv_crypto_args_t` FIELD-FOR-FIELD (same order — `repr(C)`
/// layout is declaration-order-dependent, and this crate has no shared codegen with the C header,
/// so this hand-mirrored copy is the ABI's Rust-side half; if the two ever disagree, that is a bug
/// in whichever file was not updated, same posture `rp2350-abi`'s own doc comment states for its
/// hand-mirrored error/custody codes).
#[repr(C)]
struct SvCryptoArgs {
    aad: *const u8,
    aad_len: u32,
    buf: *mut u8,
    cap: u32,
    len: u32,
}

// Frozen NS<->S veneer ABI (ground rules / `veneer.h`) — declared as ordinary external functions;
// each argument matches the C signature bit-for-bit. `unsafe extern "C"` items are inherently
// `unsafe` to call (crossing an FFI/world boundary).
extern "C" {
    fn sv_session_establish() -> i32;
    fn sv_seal(handle: u32, args: *mut SvCryptoArgs) -> i32;
    fn sv_open(handle: u32, args: *mut SvCryptoArgs) -> i32;
    fn sv_profile(identity_custody_out: *mut u32, session_custody_out: *mut u32) -> i32;
}

impl VeneerSessionCipher {
    pub fn new() -> Self {
        VeneerSessionCipher {
            cached_custody: Cell::new(None),
        }
    }
}

impl Default for VeneerSessionCipher {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionCipher for VeneerSessionCipher {
    /// Queries `sv_profile` at most once and caches the result — the profile is fixed for the
    /// veneer session's whole lifetime (Stage 4 story #355's scope has no re-keying/re-provisioning
    /// path), so re-querying on every `custody()` call would only add needless S-world round trips
    /// for a value that never changes after boot. Documented choice, not an oversight.
    fn custody(&self) -> KeyCustody {
        if let Some(c) = self.cached_custody.get() {
            return c;
        }
        let mut identity_code: u32 = 0;
        let mut session_code: u32 = 0;
        // SAFETY: `sv_profile` is the frozen veneer entry point; both out-params are plain `u32`
        // stack locals (their addresses, not slices) — the veneer's own `cmse_check_address_range`
        // validates them before writing, per the frozen ABI contract.
        let rc = unsafe {
            sv_profile(
                &mut identity_code as *mut u32,
                &mut session_code as *mut u32,
            )
        };
        let custody = if rc < 0 {
            // Fail closed to the weakest tier on a veneer failure — never silently claim stronger
            // custody than proven.
            KeyCustody::SoftwareMemory
        } else {
            custody_code_to_key_custody(session_code).unwrap_or(KeyCustody::SoftwareMemory)
        };
        self.cached_custody.set(Some(custody));
        custody
    }

    fn establish(&mut self) -> SecurityResult<KeyHandle> {
        // SAFETY: frozen veneer entry point, no pointers passed.
        let rc = unsafe { sv_session_establish() };
        if rc < 0 {
            return Err(sv_err_to_security_error(rc));
        }
        Ok(KeyHandle(rc as u32))
    }

    fn seal(
        &mut self,
        handle: KeyHandle,
        aad: &[u8],
        buf: &mut [u8],
        len: usize,
    ) -> SecurityResult<usize> {
        // SAFETY: `sv_seal` is the frozen veneer entry point; see `seal_or_open`'s own SAFETY note
        // for what this call touches.
        seal_or_open(|h, a| unsafe { sv_seal(h, a) }, handle, aad, buf, len)
    }

    fn open(
        &mut self,
        handle: KeyHandle,
        aad: &[u8],
        buf: &mut [u8],
        len: usize,
    ) -> SecurityResult<usize> {
        // SAFETY: `sv_open` is the frozen veneer entry point; see `seal_or_open`'s own SAFETY note
        // for what this call touches.
        seal_or_open(|h, a| unsafe { sv_open(h, a) }, handle, aad, buf, len)
    }
}

/// Shared fail-closed plumbing for `seal`/`open` — enforces the frozen `SV_MAX_PAYLOAD` cap (see
/// this file's module doc) before ever crossing the veneer boundary, builds the [`SvCryptoArgs`]
/// the corrected 2-argument ABI expects, and maps a negative return to [`SecurityError`] via
/// `rp2350_abi::sv_err_to_security_error`.
fn seal_or_open(
    veneer: impl FnOnce(u32, *mut SvCryptoArgs) -> i32,
    handle: KeyHandle,
    aad: &[u8],
    buf: &mut [u8],
    len: usize,
) -> SecurityResult<usize> {
    if len > buf.len() {
        // A `len` longer than the buffer that supposedly holds it is malformed input, not an
        // undersized *output* buffer — same distinction `SessionCipher::open`'s doc draws.
        return Err(SecurityError::Malformed);
    }
    let cap = buf.len();
    // Fail closed rather than silently truncate/chunk: SV_MAX_PAYLOAD is the S-side static
    // scratch's fixed size (frozen ABI) — a request needing more room than that cannot be served
    // today (chunking is TODO(#355)).
    if cap > SV_MAX_PAYLOAD as usize || len > SV_MAX_PAYLOAD as usize {
        return Err(SecurityError::BufferTooSmall);
    }
    if aad.len() > u32::MAX as usize {
        return Err(SecurityError::Malformed);
    }

    // Built on the stack, then handed to the veneer by pointer for the single call below — never
    // stored, never outliving this function. `buf`/`aad` are caller-owned live Rust slices (not
    // raw pointers conjured from nothing); the veneer performs its own `cmse_check_address_range`
    // validation on `args` itself AND on `args->aad`/`args->buf` before dereferencing either
    // (frozen ABI contract — see `../../veneers/veneer.c`) and copies into S-side static scratch.
    let mut args = SvCryptoArgs {
        aad: aad.as_ptr(),
        aad_len: aad.len() as u32,
        buf: buf.as_mut_ptr(),
        cap: cap as u32,
        len: len as u32,
    };
    let rc = veneer(handle.0, &mut args as *mut SvCryptoArgs);
    if rc < 0 {
        return Err(sv_err_to_security_error(rc));
    }
    Ok(rc as usize)
}

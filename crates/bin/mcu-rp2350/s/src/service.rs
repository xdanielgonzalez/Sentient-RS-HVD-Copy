//! The S-side session-cipher service the veneers (`../veneers/veneer.c`) call into via the
//! `rust_sv_*` `extern "C"` functions. Holds one [`SoftwareSessionStore`] behind a single
//! `critical_section::Mutex` — sound only for this scaffold's single-consumer, synchronous access
//! (CLAUDE.md's "Concurrency on `static` state" rule) — and bridges `Result<_, SecurityError>` to
//! the frozen `i32` ABI via `rp2350_abi::security_error_to_sv_err`. Entropy is DEMO-ONLY, not
//! cryptographically secure: [`SoftwareSessionStore::establish`] seeds from the `DWT` cycle
//! counter, not a hardware TRNG — TODO(#355) wire a real source first.
use critical_section::Mutex;
use lib_security::backends::software::{InsecureTestRng, SoftwareSessionStore};
use lib_security::{KeyHandle, SecurityError, SecurityProfile, SessionCipher};
use rp2350_abi::security_error_to_sv_err;

use core::cell::RefCell;

/// The single S-side session store, guarded by one critical section. `RefCell` inside the
/// `Mutex` because [`SessionCipher::seal`]/[`open`]/[`establish`] all need `&mut self` — the
/// `critical_section::Mutex` only hands back a `&SoftwareSessionStore` for the lock's lifetime, so
/// interior mutability is required to actually call them. Every access is a single, short
/// (bounded-work) critical section — no I/O, no unbounded loop, inside the lock.
static SESSION_STORE: Mutex<RefCell<Option<SoftwareSessionStore<InsecureTestRng>>>> =
    Mutex::new(RefCell::new(None));

/// Seed [`SESSION_STORE`] from the DWT cycle counter (see this module's doc for why that is a
/// demo-only, non-cryptographic entropy source). Must run exactly once, before any `sv_*` veneer
/// call reaches this service — `s`'s `main` calls this right after [`crate::sau::configure`].
///
/// # Safety
/// Must be called exactly once, before any veneer call, and before interrupts/exceptions that
/// might themselves call into this service are enabled (there are none in this scaffold's `main`,
/// but future additions must preserve that ordering).
pub unsafe fn init(core: &mut cortex_m::Peripherals) {
    // DEMCR.TRCENA must be set before DWT.CYCCNTENA has any effect (Armv8-M debug/trace block).
    core.DCB.enable_trace();
    core.DWT.enable_cycle_counter();
    let seed = u64::from(cortex_m::peripheral::DWT::cycle_count())
        // Mix in a fixed, non-secret salt so an all-zero cycle count (e.g. a cold debugger attach
        // that resets DWT to 0) still doesn't seed the demo RNG with literal zero.
        ^ 0x5EC1_D5E1_ED5E_ED00_u64;

    critical_section::with(|cs| {
        SESSION_STORE
            .borrow(cs)
            .replace(Some(SoftwareSessionStore::new(InsecureTestRng::new(seed))));
    });
}

#[no_mangle]
pub extern "C" fn rust_sv_establish() -> i32 {
    critical_section::with(|cs| {
        let mut slot = SESSION_STORE.borrow(cs).borrow_mut();
        let Some(store) = slot.as_mut() else {
            return security_error_to_sv_err(SecurityError::Backend);
        };
        match store.establish() {
            Ok(KeyHandle(h)) => h as i32,
            Err(e) => security_error_to_sv_err(e),
        }
    })
}

/// # Safety
/// `buf` must be valid for reads and writes of `cap` bytes, and the first `len` bytes must be
/// initialized plaintext. Upheld by the caller: `../veneers/veneer.c` only ever passes its own
/// S-side static scratch buffer here (never a raw NS pointer), already copy-in'd and sized.
#[no_mangle]
pub unsafe extern "C" fn rust_sv_seal(
    handle: u32,
    aad_ptr: *const u8,
    aad_len: u32,
    buf_ptr: *mut u8,
    cap: u32,
    len: u32,
) -> i32 {
    // SAFETY: caller contract above.
    let aad = unsafe { core::slice::from_raw_parts(aad_ptr, aad_len as usize) };
    // SAFETY: caller contract above.
    let buf = unsafe { core::slice::from_raw_parts_mut(buf_ptr, cap as usize) };

    critical_section::with(|cs| {
        let mut slot = SESSION_STORE.borrow(cs).borrow_mut();
        let Some(store) = slot.as_mut() else {
            return security_error_to_sv_err(SecurityError::Backend);
        };
        match store.seal(KeyHandle(handle), aad, buf, len as usize) {
            Ok(new_len) => new_len as i32,
            Err(e) => security_error_to_sv_err(e),
        }
    })
}

/// # Safety
/// Same contract as [`rust_sv_seal`]: `buf` is the S-side scratch buffer, valid for `cap` bytes,
/// with `len` initialized bytes of framed ciphertext.
#[no_mangle]
pub unsafe extern "C" fn rust_sv_open(
    handle: u32,
    aad_ptr: *const u8,
    aad_len: u32,
    buf_ptr: *mut u8,
    cap: u32,
    len: u32,
) -> i32 {
    // SAFETY: caller contract above.
    let aad = unsafe { core::slice::from_raw_parts(aad_ptr, aad_len as usize) };
    // SAFETY: caller contract above.
    let buf = unsafe { core::slice::from_raw_parts_mut(buf_ptr, cap as usize) };

    critical_section::with(|cs| {
        let mut slot = SESSION_STORE.borrow(cs).borrow_mut();
        let Some(store) = slot.as_mut() else {
            return security_error_to_sv_err(SecurityError::Backend);
        };
        match store.open(KeyHandle(handle), aad, buf, len as usize) {
            Ok(new_len) => new_len as i32,
            Err(e) => security_error_to_sv_err(e),
        }
    })
}

/// # Safety
/// `identity_custody_out`/`session_custody_out` must each be valid for one `u32` write. Upheld by
/// the caller: `../veneers/veneer.c` only calls this with pointers to its own S-side stack locals.
#[no_mangle]
pub unsafe extern "C" fn rust_sv_profile(
    identity_custody_out: *mut u32,
    session_custody_out: *mut u32,
) -> i32 {
    // Reported from the compile-time `SecurityProfile`, not a live store lookup — see
    // `../veneers/veneer.h`'s doc on why: this scaffold's veneer surface has no identity-key
    // operation, so there is no instantiated identity `KeyStore` to ask; the profile is the
    // honest, build-time-fixed answer for both purposes.
    let profile = SecurityProfile::active();
    let identity_code = rp2350_abi::key_custody_to_custody_code(profile.identity_custody);
    let session_code = rp2350_abi::key_custody_to_custody_code(profile.session_custody);

    // SAFETY: caller contract above.
    unsafe {
        core::ptr::write(identity_custody_out, identity_code);
        core::ptr::write(session_custody_out, session_code);
    }
    0
}

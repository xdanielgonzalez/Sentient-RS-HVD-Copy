/* veneer.h — the FROZEN NS<->S ABI for the RP2350 TrustZone-M demo scaffold (EPIC #350, story S4
 * #355). THIS IS A DEMO SCAFFOLD, NOT PRODUCTION: claims limited to signed boot / load-time
 * integrity / IP protection vs non-physical attackers. NO FIPS, NO attestation, NO anti-tamper.
 * RP2350 A4 stepping only.
 *
 * Four `cmse_nonsecure_entry` veneers, compiled `-mcmse` (see ../s/build.rs). Every veneer
 * validates any NS pointer argument with `cmse_check_address_range` before touching it, then
 * copies into an S-side static scratch buffer (copy-in), calls into the Rust S-side service
 * (`rust_sv_*`, implemented in `../s/src/service.rs`), and copies the result back out
 * (copy-out) — see veneer.c for the exact sequence. Kept DUMB: one-shot passthrough, no
 * loops/retry/policy (CLAUDE.md's seam doctrine).
 *
 * Payload cap: SV_MAX_PAYLOAD bytes per call. A larger payload is refused with SV_ERR_CAPACITY.
 * Chunking a payload across multiple calls is TODO(#355), not implemented here.
 *
 * The error codes and SV_MAX_PAYLOAD below are a HAND-mirrored copy of
 * `../abi/src/types/sv_error.rs` / `../abi/src/lib.rs` (this scaffold has no cbindgen step —
 * TODO). If this header and that crate ever disagree, that is a bug in whichever file was not
 * updated; the Rust crate is authoritative.
 *
 * ## CORRECTION to the originally-specified sv_seal/sv_open signatures (verified, not guessed)
 * The story brief specified `sv_seal`/`sv_open` as 6 plain arguments
 * (`handle, aad, aad_len, buf, cap, len`). Building that EXACTLY as given fails: ARMv8-M's
 * `cmse_nonsecure_entry` attribute requires every argument to fit in the AAPCS integer/pointer
 * argument registers `r0`-`r3` (max 4 words) — a secure gateway veneer cannot marshal
 * stack-spilled arguments across the security-state boundary. `arm-none-eabi-gcc 13.2.1` rejects
 * the 6-argument form with: `'cmse_nonsecure_entry' attribute not available to functions with
 * arguments passed on the stack` (confirmed by actually building it — see ../s/build.rs's gate;
 * this is the "get the datum, don't guess" oracle, not an assumption). `sv_seal`/`sv_open` are
 * revised below to `(uint32_t handle, sv_crypto_args_t *args)` — 2 register arguments — bundling
 * the other four into one NS-pointer-passed struct. `sv_session_establish` (0 args) and
 * `sv_profile` (2 pointer args) already fit and are UNCHANGED from the original brief.
 */
#ifndef SENTIENT_RP2350_VENEER_H
#define SENTIENT_RP2350_VENEER_H

#include <stdint.h>

/* --- frozen sv_* error codes (negative i32) --- mirrors abi/src/types/sv_error.rs::SvError --- */
#define SV_ERR_NOT_IMPLEMENTED     (-1) /* SvError::NotImplemented */
#define SV_ERR_KEY_NOT_FOUND       (-2) /* SvError::KeyNotFound */
#define SV_ERR_VERIFICATION_FAILED (-3) /* SvError::VerificationFailed */
#define SV_ERR_BUFFER_TOO_SMALL    (-4) /* SvError::BufferTooSmall */
#define SV_ERR_CAPACITY            (-5) /* SvError::Capacity */
#define SV_ERR_BAD_POINTER         (-6) /* SvError::BadPointer */
#define SV_ERR_BACKEND             (-7) /* SvError::Backend */

/* --- frozen sv_profile custody codes (u32) --- mirrors abi/src/types/sv_custody.rs::SvCustody -- */
#define SV_CUSTODY_SOFTWARE_MEMORY 0u /* SvCustody::SoftwareMemory */
#define SV_CUSTODY_SECURE_WORLD    1u /* SvCustody::SecureWorld */
#define SV_CUSTODY_SECURE_ELEMENT  2u /* SvCustody::SecureElement */

/* Max payload sv_seal/sv_open accept per call, in bytes (frozen ABI constant). */
#define SV_MAX_PAYLOAD 4096u

/* sv_seal/sv_open's arguments beyond `handle`, bundled into one NS-pointer-passed struct — see this
 * header's "CORRECTION" note above for why. `args` itself is validated with
 * cmse_check_address_range before any field is read; `aad`/`buf` are then validated AGAIN
 * (validating the struct's own memory says nothing about what its pointer FIELDS point to). */
typedef struct {
    const uint8_t *aad;
    uint32_t aad_len;
    uint8_t *buf;
    uint32_t cap;
    uint32_t len;
} sv_crypto_args_t;

/* Establish a new session key inside the S-side store. Returns >= 0 (a handle) on success, or a
 * negative SV_ERR_* on failure (SV_ERR_BACKEND if the key arena is exhausted). No NS pointers. */
__attribute__((cmse_nonsecure_entry))
int32_t sv_session_establish(void);

/* Seal args->buf[..args->len] (plaintext) in place under the session key `handle`, authenticating
 * args->aad alongside it. On success args->buf holds [nonce][ciphertext][tag] and the return value
 * is that framed length (args->len + 28 for this build's AES-256-GCM backend); args->buf must have
 * capacity args->cap for it, or this returns SV_ERR_BUFFER_TOO_SMALL. args->aad_len must not
 * exceed this scaffold's fixed internal AAD scratch (64 bytes; not part of the frozen ABI, an
 * implementation limit of this demo) or this returns SV_ERR_CAPACITY. An unknown handle is
 * SV_ERR_KEY_NOT_FOUND; an invalid/oversized `args` pointer, or an invalid/oversized
 * args->aad/args->buf, is SV_ERR_BAD_POINTER. */
__attribute__((cmse_nonsecure_entry))
int32_t sv_seal(uint32_t handle, sv_crypto_args_t *args);

/* Reverse sv_seal: authenticate and decrypt the framed args->buf[..args->len] under `handle` and
 * args->aad, overwriting args->buf with the recovered plaintext and returning its length
 * (args->len - 28). A tampered ciphertext/tag is SV_ERR_VERIFICATION_FAILED; an args->len too
 * short to hold the framing is SV_ERR_BUFFER_TOO_SMALL (the framing itself doesn't fit — a
 * distinct condition from sv_seal's output-capacity check, but this ABI folds both to the same
 * code; see abi's error mapping doc for why). An unknown handle is SV_ERR_KEY_NOT_FOUND; an
 * invalid/oversized `args` pointer, or an invalid/oversized args->aad/args->buf, is
 * SV_ERR_BAD_POINTER; an args->aad_len over this scaffold's fixed AAD scratch is SV_ERR_CAPACITY. */
__attribute__((cmse_nonsecure_entry))
int32_t sv_open(uint32_t handle, sv_crypto_args_t *args);

/* Report this build's key-custody tier for the identity and session key purposes (SV_CUSTODY_*),
 * derived from lib_security::SecurityProfile::active() (compile-time feature toggles) — NOT from
 * any actually-instantiated identity key store (this scaffold's veneer surface has no
 * identity-key operation; see abi's crate doc). Returns 0 on success, or a negative SV_ERR_* (only
 * SV_ERR_BAD_POINTER is reachable here, for an invalid NS out-pointer). */
__attribute__((cmse_nonsecure_entry))
int32_t sv_profile(uint32_t *identity_custody_out, uint32_t *session_custody_out);

#endif /* SENTIENT_RP2350_VENEER_H */

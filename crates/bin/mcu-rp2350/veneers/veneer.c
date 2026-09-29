/* veneer.c — implements the frozen sv_* ABI (see veneer.h). Compiled -mcmse by ../s/build.rs.
 *
 * Every entry point: (1) bound-checks against this build's fixed scratch capacities, (2) validates
 * every NS pointer argument with cmse_check_address_range BEFORE dereferencing it, (3) copies into
 * S-side static scratch (copy-in), (4) calls the Rust S-side service (rust_sv_*, ../s/src/
 * service.rs) which touches ONLY that already-validated S-side memory, (5) copies the result back
 * out into the (already-validated) NS buffer (copy-out). Kept DUMB on purpose: no loops, no retry,
 * no policy here — that discipline lives in the Rust service / lib-security seam (CLAUDE.md's
 * "keep the FFI/C shim DUMB" rule).
 *
 * DEMO SCAFFOLD, NOT PRODUCTION. RP2350 A4 stepping only.
 */
#include <arm_cmse.h>
#include <stddef.h>
#include <string.h>

#include "veneer.h"

/* --- S-side static scratch --- never a per-call stack local (this repo's embedded stack-
 * discipline rule: large buffers live in static/reused storage, not per-call frame locals). Sized
 * SV_MAX_PAYLOAD + 64: the AES-256-GCM backend's framing overhead is 12-byte nonce + 16-byte tag =
 * 28 bytes; 64 is a rounded-up safety margin, not a tight fit. */
static uint8_t sv_scratch[SV_MAX_PAYLOAD + 64u];

/* AAD scratch cap. NOT part of the frozen ABI (aad_len itself carries no cap in the frozen
 * signature) — this is an internal, documented limit of THIS demo scaffold's fixed S-side buffer.
 * Chunking a larger AAD is TODO(#355), same as payload chunking. */
#define SV_MAX_AAD 64u
static uint8_t sv_aad_scratch[SV_MAX_AAD];

/* Implemented in Rust (../s/src/service.rs). These operate ONLY on the already-validated,
 * already-copied S-side scratch above — they are never handed a raw NS pointer. */
extern int32_t rust_sv_establish(void);
extern int32_t rust_sv_seal(uint32_t handle, const uint8_t *aad, uint32_t aad_len,
                             uint8_t *buf, uint32_t cap, uint32_t len);
extern int32_t rust_sv_open(uint32_t handle, const uint8_t *aad, uint32_t aad_len,
                             uint8_t *buf, uint32_t cap, uint32_t len);
extern int32_t rust_sv_profile(uint32_t *identity_custody_out, uint32_t *session_custody_out);

__attribute__((cmse_nonsecure_entry))
int32_t sv_session_establish(void)
{
    /* No NS pointers involved — nothing to validate or copy. */
    return rust_sv_establish();
}

__attribute__((cmse_nonsecure_entry))
int32_t sv_seal(uint32_t handle, sv_crypto_args_t *args)
{
    /* `args` itself is the first thing to validate — everything else is read out of NS memory
     * through it, so nothing past this point may touch `args`'s fields until this passes. */
    if (args == NULL ||
        cmse_check_address_range(args, sizeof(sv_crypto_args_t), CMSE_NONSECURE) == NULL) {
        return SV_ERR_BAD_POINTER;
    }
    /* Copy the small, fixed-size struct itself into an S-side local (NOT the big payload
     * scratch — this repo's stack-locals rule targets large/per-iteration buffers, a 5-word
     * struct is fine) so a racing NS write to the original can't change `aad`/`buf`/`cap`/`len`
     * out from under the checks below (the same copy-in discipline this file uses for the
     * payload/AAD themselves, applied one level up). */
    sv_crypto_args_t a = *args;

    /* Fail closed BEFORE touching any pointer: bound checks against this build's fixed capacities.
     * A larger payload/AAD is refused, never silently truncated. */
    if (a.len > SV_MAX_PAYLOAD || a.cap > sizeof(sv_scratch)) {
        return SV_ERR_CAPACITY;
    }
    if (a.aad_len > SV_MAX_AAD) {
        return SV_ERR_CAPACITY;
    }

    if (a.buf == NULL || cmse_check_address_range(a.buf, a.cap, CMSE_NONSECURE) == NULL) {
        return SV_ERR_BAD_POINTER;
    }
    if (a.len > a.cap) {
        return SV_ERR_BUFFER_TOO_SMALL;
    }
    if (a.aad_len > 0u) {
        if (a.aad == NULL ||
            cmse_check_address_range((void *)a.aad, a.aad_len, CMSE_NONSECURE) == NULL) {
            return SV_ERR_BAD_POINTER;
        }
    }

    /* Copy-in: NS memory is not touched again after this point. */
    memcpy(sv_scratch, a.buf, a.len);
    if (a.aad_len > 0u) {
        memcpy(sv_aad_scratch, a.aad, a.aad_len);
    }

    int32_t result = rust_sv_seal(handle, sv_aad_scratch, a.aad_len, sv_scratch,
                                   (uint32_t)sizeof(sv_scratch), a.len);
    if (result < 0) {
        return result;
    }

    uint32_t new_len = (uint32_t)result;
    if (new_len > a.cap) {
        /* The sealed output does not fit the NS caller's own buffer capacity. */
        return SV_ERR_BUFFER_TOO_SMALL;
    }

    /* Copy-out. */
    memcpy(a.buf, sv_scratch, new_len);
    return (int32_t)new_len;
}

__attribute__((cmse_nonsecure_entry))
int32_t sv_open(uint32_t handle, sv_crypto_args_t *args)
{
    if (args == NULL ||
        cmse_check_address_range(args, sizeof(sv_crypto_args_t), CMSE_NONSECURE) == NULL) {
        return SV_ERR_BAD_POINTER;
    }
    sv_crypto_args_t a = *args;

    if (a.len > sizeof(sv_scratch) || a.cap > sizeof(sv_scratch)) {
        return SV_ERR_CAPACITY;
    }
    if (a.aad_len > SV_MAX_AAD) {
        return SV_ERR_CAPACITY;
    }

    if (a.buf == NULL || cmse_check_address_range(a.buf, a.cap, CMSE_NONSECURE) == NULL) {
        return SV_ERR_BAD_POINTER;
    }
    if (a.len > a.cap) {
        return SV_ERR_BUFFER_TOO_SMALL;
    }
    if (a.aad_len > 0u) {
        if (a.aad == NULL ||
            cmse_check_address_range((void *)a.aad, a.aad_len, CMSE_NONSECURE) == NULL) {
            return SV_ERR_BAD_POINTER;
        }
    }

    memcpy(sv_scratch, a.buf, a.len);
    if (a.aad_len > 0u) {
        memcpy(sv_aad_scratch, a.aad, a.aad_len);
    }

    int32_t result = rust_sv_open(handle, sv_aad_scratch, a.aad_len, sv_scratch,
                                  (uint32_t)sizeof(sv_scratch), a.len);
    if (result < 0) {
        return result;
    }

    uint32_t new_len = (uint32_t)result;
    if (new_len > a.cap) {
        return SV_ERR_BUFFER_TOO_SMALL;
    }

    memcpy(a.buf, sv_scratch, new_len);
    return (int32_t)new_len;
}

__attribute__((cmse_nonsecure_entry))
int32_t sv_profile(uint32_t *identity_custody_out, uint32_t *session_custody_out)
{
    if (identity_custody_out == NULL ||
        cmse_check_address_range(identity_custody_out, sizeof(uint32_t), CMSE_NONSECURE) == NULL) {
        return SV_ERR_BAD_POINTER;
    }
    if (session_custody_out == NULL ||
        cmse_check_address_range(session_custody_out, sizeof(uint32_t), CMSE_NONSECURE) == NULL) {
        return SV_ERR_BAD_POINTER;
    }

    /* Compute into S-side locals (fixed 4-byte words, not the big payload scratch — this repo's
     * stack-locals rule targets large/per-iteration buffers, not two words) before the single
     * copy-out through each already-validated NS pointer. */
    uint32_t identity_custody = 0;
    uint32_t session_custody = 0;
    int32_t result = rust_sv_profile(&identity_custody, &session_custody);
    if (result < 0) {
        return result;
    }

    *identity_custody_out = identity_custody;
    *session_custody_out = session_custody;
    return 0;
}

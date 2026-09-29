//! SAU (Security Attribution Unit) configuration: the ARMv8-M core peripheral (base `0xE000EDD0`,
//! architectural, not RP2350-specific) that partitions the address space into Secure / NSC / NS.
//! Three regions match `../memory.x` / `mem_layout.rs`: 0 = NS flash, 1 = NS RAM, 2 = the NSC
//! veneer subregion (`RLAR.NSC`). `SAU_CTRL.ALLNS = 0` so anything not covered defaults Secure
//! (fail-closed). TODO(#355): the SAU gates only core CPU access, not DMA/other bus masters;
//! RP2350's `ACCESSCTRL` is the likely fix but unverified — a misbehaving NS DMA could reach S memory.
use core::ptr::{read_volatile, write_volatile};

use crate::mem_layout::{
    NSC_BASE, NS_FLASH_BASE, NS_FLASH_END, NS_RAM_BASE, NS_RAM_END, S_FLASH_END,
};

const SAU_CTRL: *mut u32 = 0xE000_EDD0 as *mut u32;
const SAU_TYPE: *const u32 = 0xE000_EDD4 as *const u32;
const SAU_RNR: *mut u32 = 0xE000_EDD8 as *mut u32;
const SAU_RBAR: *mut u32 = 0xE000_EDDC as *mut u32;
const SAU_RLAR: *mut u32 = 0xE000_EDE0 as *mut u32;

/// `SAU_CTRL` bit 0. Bit 1 (`ALLNS`) is deliberately left at its reset value of 0 everywhere this
/// file writes `SAU_CTRL` — fail closed: with `ALLNS = 0`, any address not covered by an enabled
/// region below stays Secure. (No `SAU_CTRL_ALLNS` constant is defined since nothing here ever sets
/// it — an unused "always 0" bit constant would just be dead code.)
const SAU_CTRL_ENABLE: u32 = 1 << 0;

const SAU_RLAR_ENABLE: u32 = 1 << 0;
const SAU_RLAR_NSC: u32 = 1 << 1;

/// ARMv8-M SAU region granularity: base and limit addresses are specified by their bits `[31:5]`
/// (a 32-byte alignment requirement) — architectural, not RP2350-specific.
const SAU_REGION_ALIGN: u32 = 32;

/// Program one SAU region: `[base, limit)` (limit exclusive on this fn's own interface; SAU_RLAR
/// itself encodes an INCLUSIVE last address, so this subtracts 1 and relies on `limit` being
/// region-aligned — every caller below passes an aligned `limit`). `nsc` sets `RLAR.NSC`.
///
/// # Safety
/// Caller must ensure `region` is a valid SAU region index for this core (checked against
/// `SAU_TYPE` in [`configure`]) and that `base`/`limit` are `SAU_REGION_ALIGN`-aligned.
unsafe fn program_region(region: u32, base: u32, limit: u32, nsc: bool) {
    debug_assert_eq!(
        base % SAU_REGION_ALIGN,
        0,
        "SAU base must be 32-byte aligned"
    );
    debug_assert_eq!(
        limit % SAU_REGION_ALIGN,
        0,
        "SAU limit must be 32-byte aligned"
    );

    write_volatile(SAU_RNR, region);
    write_volatile(SAU_RBAR, base);
    let mut rlar = (limit - SAU_REGION_ALIGN) | SAU_RLAR_ENABLE;
    if nsc {
        rlar |= SAU_RLAR_NSC;
    }
    write_volatile(SAU_RLAR, rlar);
}

/// Configure the SAU per `memory.x`'s split, then enable it with `ALLNS = 0` (fail closed:
/// anything not explicitly carved out below stays Secure).
///
/// # Safety
/// Must run once, early in `s`'s `main`, before any NS code executes and before the `bxns` handoff
/// (`ns_jump::jump_to_ns`) — programming SAU regions after NS code is already running would let it
/// observe an inconsistent security map mid-update.
pub unsafe fn configure() {
    // Defensive check against this core's actual implemented-region count (`SAU_TYPE.SREGION`,
    // bits [7:0]) before writing region indices 0..=2 — fails loudly (panic, this is boot-time
    // init code where a panic is the correct fail-closed response) rather than silently writing
    // past what the core implements.
    let implemented_regions = read_volatile(SAU_TYPE) & 0xFF;
    assert!(
        implemented_regions >= 3,
        "SAU implements fewer regions than this scaffold's 3-region config needs"
    );

    // Region 0: NS flash.
    program_region(0, NS_FLASH_BASE, NS_FLASH_END, false);
    // Region 1: NS RAM.
    program_region(1, NS_RAM_BASE, NS_RAM_END, false);
    // Region 2: the NSC veneer subregion — the ONLY way NS code reaches S code.
    program_region(2, NSC_BASE, S_FLASH_END, true);

    // Enable the SAU. ALLNS = 0: any address NOT covered by an enabled region above is Secure.
    write_volatile(SAU_CTRL, SAU_CTRL_ENABLE);
}

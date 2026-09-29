//! Secure-to-Non-secure handoff: point the NS-banked `VTOR`/`MSP` at the NS image, then `bxns`
//! into its Reset handler — runs once, after [`crate::sau::configure`], as the last thing `s`'s
//! `main` does, and never returns. ARMv8-M facts relied on (not RP2350-specific): NS-banked SCB
//! aliases sit at `+0x0200_0000` (`VTOR` `0xE000_ED08` -> `VTOR_NS` `0xE002_ED08`); `MSR MSP_NS, Rn`
//! sets the NS stack pointer without a state switch (inline `asm!` here, not `arm_acle`); `BXNS`
//! branches and switches to Non-secure state in one step. RP2350 facts live in `mem_layout.rs` / `sau.rs`.
use crate::mem_layout::NS_FLASH_BASE;

/// NS-banked alias of `SCB->VTOR` (Secure-view `VTOR` at `0xE000_ED08`, plus the ARMv8-M `+
/// 0x0200_0000` NS-alias offset). Written here so the exception/interrupt vector table the NS core
/// uses after the jump is the NS image's own, not the S image's.
const VTOR_NS: *mut u32 = 0xE002_ED08 as *mut u32;

/// Read the NS image's initial MSP and Reset-handler entry point out of its vector table (the
/// first two words at `NS_FLASH_BASE`, standard Cortex-M vector-table layout: word 0 = initial
/// SP, word 1 = Reset handler address), point `VTOR_NS` at that table, load `MSP_NS`, and `bxns`
/// into the entry point. Never returns.
///
/// # Safety
/// Caller must ensure: (1) [`crate::sau::configure`] has already run, so `NS_FLASH_BASE` is
/// SAU-mapped Non-secure and therefore a legal `BXNS` target; (2) a valid NS image (vector table +
/// Reset handler) is actually present at `NS_FLASH_BASE` (this scaffold does not itself verify a
/// signature or checksum over it before jumping — see `../README.md`'s claims/non-claims: signed
/// **boot** here means the *S image itself* was loaded by a signature-checked boot chain, not that
/// this function re-verifies the NS image a second time before the handoff).
pub unsafe fn jump_to_ns() -> ! {
    let vector_table = NS_FLASH_BASE as *const u32;
    let ns_msp = core::ptr::read_volatile(vector_table);
    let ns_entry = core::ptr::read_volatile(vector_table.add(1));

    core::ptr::write_volatile(VTOR_NS, NS_FLASH_BASE);

    core::arch::asm!(
        "msr msp_ns, {msp}",
        "bxns {entry}",
        msp = in(reg) ns_msp,
        entry = in(reg) ns_entry,
        options(noreturn),
    );
}

//! MPU_NS region setup — always compiled (no feature gate).
//!
//! Embassy's executor here runs every task cooperatively on ONE stack, so this MPU region does
//! NOT create enforced per-task privilege separation. What it DOES provide: fault containment for
//! the parser's static working-buffer pool ([`crate::types::ParserBufferPool`]) — marking the
//! region `XN` turns a corrupted-return-address jump landing in this buffer into an immediate
//! usage fault instead of executing garbage. The real per-task isolation boundary in this
//! scaffold is the channel (`MQTT_CHANNEL` in `main.rs`), not the MPU — see `core1.rs`.
//!
//! **Demo scaffold**: signed-boot / load-time-integrity / fault-containment posture, not a
//! FIPS/attestation/anti-tamper claim. RP2350A A4 stepping only.
//!
//! Raw PMSAv8 (Armv8-M) MPU register access via `cortex_m::peripheral::MPU` (`RBAR`/`RLAR`,
//! verified against cortex-m 0.7.7's `armv8m`-cfg `RegisterBlock`). `MPU::PTR` is `0xE000_ED90`.
//! No third-party MPU helper crate needed for this single-region setup.

use cortex_m::peripheral::MPU;

/// The one PMSAv8 MPU region number this scaffold programs. Never contends with anything else for
/// a region slot — this is the only region this crate sets up.
const NS_PARSER_REGION: u32 = 0;

/// Program the PMSAv8 `NS_PARSER` MPU region over `base..base+len` (the parser buffer pool address
/// range: [`crate::types::ParserBufferPool::base_addr`]/[`crate::types::ParserBufferPool::pool_len`])
/// and enable the MPU with the privileged-default background region ON (`PRIVDEFENA`) — background
/// memory outside this one region keeps its normal privileged-default access, matching the
/// module doc's honesty note that this scaffold touches nothing else in the image.
///
/// # Safety
/// Must be called at most once, before any task that touches `base..base+len` has started
/// (matches [`crate::types::EmbassyClock::new`]'s single-init-before-multithreading discipline) —
/// reprogramming a live region while another access is in flight is undefined per the Armv8-M ARM.
/// `base` must be 32-byte aligned and `len` a nonzero multiple of 32 bytes (PMSAv8's minimum MPU
/// granularity); callers pass the address/length of a `#[repr(align(32))]` static
/// ([`crate::types::ParserBufferPool`]), never a hand-picked address.
pub unsafe fn init_ns_parser_region(mpu: &MPU, base: u32, len: u32) {
    debug_assert_eq!(base % 32, 0, "MPU region base must be 32-byte aligned");
    debug_assert_eq!(
        len % 32,
        0,
        "MPU region length must be a multiple of 32 bytes"
    );
    debug_assert!(len > 0, "MPU region length must be nonzero");

    let limit = base.wrapping_add(len).wrapping_sub(1);

    // SAFETY (register writes below): `mpu` is a live reference to the singleton MPU peripheral
    // (obtained once, in `main`, via `cortex_m::Peripherals::take()`), so these addresses are the
    // real MPU register block. The whole sequence (RNR select -> RBAR/RLAR program -> enable) is
    // the standard PMSAv8 single-region setup; the caller's contract (see this fn's `# Safety`
    // section) covers the "at most once, before use" ordering requirement.
    unsafe {
        // MAIR0 index 0: Normal memory, Write-Back/Read-Allocate/Write-Allocate (both inner and
        // outer) — 0xFF is the standard PMSAv8 encoding for that attribute (Armv8-M ARM B3.5.6).
        // The region below selects this index via `AttrIndx = 0`.
        mpu.mair[0].write(0x0000_00FF);

        mpu.rnr.write(NS_PARSER_REGION);
        // RBAR: BASE[31:5] | SH[4:3] = 0b00 (non-shareable — single core touches this region) |
        // AP[2:1] = 0b01 (read/write, any privilege level — this image runs everything
        // privileged, so AP alone buys no cross-task isolation; see the module doc) |
        // XN[0] = 1 (never executable — the real containment value here: a data buffer must never
        // be instruction-fetched).
        mpu.rbar.write((base & !0x1F) | 0b0000_0011);
        // RLAR: LIMIT[31:5] | AttrIndx[3:1] = 0b000 | EN[0] = 1 (region enabled).
        mpu.rlar.write((limit & !0x1F) | 0b0000_0001);
        // MPU_CTRL: ENABLE(bit0)=1, HFNMIENA(bit1)=0 (fault/NMI handlers keep the reset-default
        // behavior — this scaffold takes no position on fault-handler memory access),
        // PRIVDEFENA(bit2)=1 (background region still grants privileged code its normal
        // full-system access outside this one region).
        mpu.ctrl.write(0b101);
    }

    // Ensure the region programming is visible/effective before any subsequent access relies on
    // it (required after reprogramming MPU state per the Armv8-M ARM).
    cortex_m::asm::dsb();
    cortex_m::asm::isb();
}

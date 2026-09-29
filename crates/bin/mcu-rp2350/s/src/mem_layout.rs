//! The S/NS memory split as plain constants — the single Rust-side source of truth `sau.rs` (SAU
//! region programming) and `ns_jump.rs` (the NS vector-table/entry lookup) both consume, kept in
//! lockstep with `memory.x`'s `MEMORY` block and comment (which documents the derivation and the
//! required `ns/memory.x` change — Agent B's file, not edited here).
//!
//! Not a `types/` split (CLAUDE.md's type-organization rule targets `struct`/`enum`/`trait`/alias
//! definitions; these are plain data constants, not a type).

/// Base of this image's own (S) flash region — also where `memory.x`'s `FLASH` origin sits.
pub const S_FLASH_BASE: u32 = 0x1000_0000;
/// End (exclusive) of the S flash region == start of NS flash.
pub const S_FLASH_END: u32 = 0x1002_0000;

/// Size of the NSC (Non-Secure Callable) veneer subregion carved from the END of S flash — the
/// last 4 KiB of S flash, where the `-mcmse` link step places `.gnu.sgstubs` (the SG veneer
/// trampolines). Must stay 32-byte aligned (ARMv8-M SAU minimum region granularity) and inside the
/// S flash region above — `4 KiB` satisfies both with wide margin for a scaffold this small.
pub const NSC_LEN: u32 = 0x1000;
/// Base of the NSC veneer subregion (`S_FLASH_END - NSC_LEN`).
pub const NSC_BASE: u32 = S_FLASH_END - NSC_LEN;

/// Base of the NS flash region (== `S_FLASH_END`). This is where the boot ROM's IMAGE_DEF /
/// vector-table scan for the NS image starts, and where `ns_jump.rs` reads the NS initial
/// MSP/entry-point words from.
pub const NS_FLASH_BASE: u32 = S_FLASH_END;
/// End (exclusive) of the NS flash region — end of the Feather RP2350's 8 MiB flash window
/// (WP1, delight-features, 2026-07-25: widened from the Pico 2's 4 MiB — `0x1040_0000` — when the
/// board moved to the Adafruit Feather RP2350. Same silicon; only the flash window changed. RAM,
/// below, is unaffected — flash was never this target's binding constraint).
pub const NS_FLASH_END: u32 = 0x1080_0000;

/// Base of this image's own (S) RAM region — also `memory.x`'s `RAM` origin.
pub const S_RAM_BASE: u32 = 0x2000_0000;
/// End (exclusive) of the S RAM region == start of NS RAM.
pub const S_RAM_END: u32 = 0x2001_0000;

/// Base of the NS RAM region (== `S_RAM_END`).
pub const NS_RAM_BASE: u32 = S_RAM_END;
/// End (exclusive) of the NS RAM region — this split conservatively claims a 512 KiB SRAM window
/// (matches `ns/memory.x`'s current undivided `RAM LENGTH = 512K`, i.e. `0x2000_0000 + 0x8_0000`),
/// UNCHANGED by WP1's Pico-2-to-Feather board move (RAM was, and remains, this target's binding
/// constraint — see the RP2350 Footprint & Performance research note — not something the board
/// move gave headroom on).
///
/// **Known headroom, deliberately not claimed here:** the RP2350's SRAM is actually one
/// CONTIGUOUS 520 KiB region, `0x2000_0000..=0x2008_1FFF` per the datasheet — 8 KiB more than the
/// 512 KiB this split's arithmetic assumes. That means `0x2008_0000..0x2008_2000` is real, present
/// SRAM this split does not hand to either world. Left unclaimed on purpose (WP1 is a pin/flash
/// port, not a RAM-budget change): claiming it would grow `NS_RAM_END` and move the S/NS split
/// boundary, which is out of this work package's scope. A future work package can extend
/// `NS_RAM_END` to `0x2008_2000` (and `ns/memory.x`'s `RAM LENGTH` to `0x72000`) once that's an
/// intentional decision, not a drive-by.
pub const NS_RAM_END: u32 = 0x2008_0000;

// Compile-time consistency checks tying this whole table together — catches a future edit that
// breaks the split's ordering/adjacency invariants at build time rather than at boot. These also
// incidentally exercise `S_FLASH_BASE`/`S_RAM_BASE`, which nothing else in this crate reads
// directly (only `sau.rs`/`ns_jump.rs` need the NS-side/NSC constants) — real checks, not a
// dead-code-warning workaround.
const _: () = assert!(S_FLASH_BASE < NSC_BASE);
const _: () = assert!(NSC_BASE < S_FLASH_END);
const _: () = assert!(S_FLASH_END == NS_FLASH_BASE);
const _: () = assert!(NS_FLASH_BASE < NS_FLASH_END);
const _: () = assert!(S_RAM_BASE < S_RAM_END);
const _: () = assert!(S_RAM_END == NS_RAM_BASE);
const _: () = assert!(NS_RAM_BASE < NS_RAM_END);

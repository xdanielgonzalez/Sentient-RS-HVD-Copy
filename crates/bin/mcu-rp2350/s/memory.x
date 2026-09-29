/* RP2350A (Cortex-M33) SECURE-WORLD (S) memory region — half of the S/NS TrustZone-M split for the
   demo scaffold (EPIC #350, story S4 #355). See ../README.md's split table for the full derivation
   and the corresponding change this requires in Agent B's `ns/memory.x` (NOT edited by this crate;
   reported in this story's hand-off instead).

   Split (of the Feather RP2350's 8 MiB flash @ 0x10000000 / 512 KiB SRAM @ 0x20000000 as claimed
   by this split — see `src/mem_layout.rs`'s header comment for the chip's actual 520 KiB
   contiguous SRAM and the 8 KiB left deliberately unclaimed. `ns/memory.x` documents the same
   totals, verified there against embassy-rp-0.10.0 / rp-pac-7.0.0):

   WP1 (delight-features, 2026-07-25): board moved from a Pico 2 (4 MiB flash) to an Adafruit
   Feather RP2350 (8 MiB flash) — same silicon. S flash/RAM (below) are UNCHANGED by that move;
   only NS's flash window widened (RAM was — and remains — the binding constraint, not flash).

     S  FLASH : 0x10000000 .. 0x10020000  (128 KiB)
       NSC veneer subregion : 0x1001F000 .. 0x10020000  (last 4 KiB of S flash; SAU region 2 in
       `src/sau.rs` marks this NSC via RLAR.NSC=1 — the CMSE `-mcmse` linker step places the
       `.gnu.sgstubs` non-secure-callable trampolines here)
     NS FLASH : 0x10020000 .. 0x10800000  (8064 KiB) — Agent B: `ns/memory.x`'s FLASH ORIGIN must
       move from 0x10000000 to 0x10020000 and LENGTH shrink from 8M to 0x7E0000 (8064K) to match.

     S  RAM   : 0x20000000 .. 0x20010000  (64 KiB)
     NS RAM   : 0x20010000 .. 0x20080000  (448 KiB) — Agent B: `ns/memory.x`'s RAM ORIGIN must move
       from 0x20000000 to 0x20010000 and LENGTH shrink from 512K to 0x70000 (448K) to match.

   32-byte SAU-region alignment (the ARMv8-M architectural minimum region granularity) is satisfied
   by every boundary above (all are 4 KiB-aligned or coarser).

   KNOWN GAP — documented, not invented (per this repo's "don't invent tool facts" doctrine): the
   RP2350 boot ROM requires a valid IMAGE_DEF metadata block (`.start_block`) within the first 4 KiB
   of whatever image it boots from flash base 0x10000000. Before this split, `ns/build.rs`'s comment
   records that `embassy-rp`'s own crate (feature "_rp235x") auto-generates that block just by being
   linked in. This `s` crate depends on NEITHER `embassy-rp` (no executor, no HAL — see `src/main.rs`)
   NOR a hand-authored IMAGE_DEF (asserting its exact binary layout from memory, unverified against
   the datasheet, would be exactly the kind of invented fact the doctrine forbids). Net effect: the
   ELF this crate links proves the SAU-config / veneer-service / `bxns`-handoff logic compiles and
   links correctly — it is NOT yet demonstrated to be boot-ROM-loadable on a real RP2350 or under
   Renode. That is an open item for the hardware-bring-up ledger, not this story's gate (which is
   `cargo build --release -p rp2350-s`). Options to close it before any hardware/Renode pass (not
   attempted here): (a) add a minimal `embassy-rp` dependency to `s` purely for its IMAGE_DEF static
   side effect, (b) hand-author the IMAGE_DEF block against the RP2350 datasheet's boot-ROM binary
   format once that reference is in hand, (c) reorder which image the boot ROM scans first. */
MEMORY {
    FLASH : ORIGIN = 0x10000000, LENGTH = 128K
    RAM   : ORIGIN = 0x20000000, LENGTH = 64K
}

//! `rp2350-s` — the RP2350 (Cortex-M33) secure-world (S) binary (EPIC #350, story #355). Demo
//! scaffold: signed boot / load-time integrity / IP protection only — no FIPS, attestation, or
//! anti-tamper; RP2350 A4 only (see `../README.md` for the claims list and S/NS memory-split table).
//! Boots first (flash base; see `memory.x`'s IMAGE_DEF/boot-ROM gap) and, as a bare `cortex-m-rt`
//! binary with no executor, runs [`sau::configure`] (lock NS to its flash/RAM + NSC veneer
//! regions), [`service::init`] (seed the session-cipher store), then [`ns_jump::jump_to_ns`]
//! (`bxns` into NS — never returns).
#![no_std]
#![no_main]

use cortex_m_rt::entry;
use panic_halt as _;

mod mem_layout;
mod ns_jump;
mod sau;
mod service;

#[entry]
fn main() -> ! {
    // SAFETY: this is the first and only thing `main` does before configuring the SAU and jumping
    // to NS — no NS code has run yet, no concurrent access to the SAU or the session-store statics
    // is possible this early in boot.
    unsafe {
        sau::configure();

        let mut core = cortex_m::Peripherals::take().expect("Peripherals::take called once, here");
        service::init(&mut core);

        ns_jump::jump_to_ns();
    }
}

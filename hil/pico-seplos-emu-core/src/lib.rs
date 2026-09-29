//! Pure Seplos BMS responder protocol logic — see [`seplos_responder`] for the fork/provenance
//! note. `no_std`, no `alloc`; builds for the host (`cargo test`, no `--target`) and for
//! `thumbv6m-none-eabi` (RP2040). Zero hardware/embassy dependency by design: the firmware crate
//! (`hil/pico-seplos-emu`) is the only consumer that touches real UART hardware.
#![no_std]
#![forbid(unsafe_code)]

pub mod seplos_responder;

//! `VoltronicSim` — a pure, transport-agnostic Voltronic/MPP-Solar PI30 inverter responder: request
//! bytes in, response-frame bytes out (D13). The RS-232 inverter leg's counterpart to
//! [`crate::seplos::SeplosSim`].
//!
//! Like `SeplosSim`, it has NO I/O of its own — [`VoltronicSim::respond`] is a pure function of
//! `(canned payloads, request bytes) -> Option<response frame>`, so the same core drives a host pty
//! (`test-e2e`'s `voltronic_inverter_e2e`), a socket, or real hardware later. Frame CRC-16/XMODEM is
//! **reused** from [`lib_parse::voltronic::crc`] (the single authoritative implementation shared with
//! the real decoder — never re-derived), so the sim and decoder can't silently drift.
//!
//! PROVISIONAL: the default payloads are derived from the legacy Python + protocol docs, not a real
//! Voltronic wire capture (decision 0001's open risk).

mod impl_voltronic_sim;
mod types;

pub use types::*;

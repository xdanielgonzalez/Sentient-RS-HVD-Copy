/// A pure Voltronic/MPP-Solar PI30 inverter responder — the RS-232 leg's counterpart to
/// `crate::seplos::SeplosSim` (D13). Request bytes in, response-frame bytes out; no I/O of its own
/// (no pty, no threads, no tokio), so the same core drives a host pty (`test-e2e`'s
/// `voltronic_inverter_e2e`) or real hardware later.
///
/// Holds one canned ASCII payload per command (`QPIGS`/`QMOD`/`QPIWS`/`QPIRI`/`QID`) — the response
/// frame is `(` + payload + CRC-16/XMODEM + `\r`, built with the REAL `lib_parse::voltronic::crc`
/// (never re-derived, so the sim and the decoder can't drift). Fault-injection knobs mutate the mode
/// (`QMOD`) and fault (`QPIWS`) payloads. Everything is PROVISIONAL — the default payloads are
/// derived, not from a real Voltronic wire capture (decision 0001's open risk).
#[derive(Debug, Clone)]
pub struct VoltronicSim {
    pub(in crate::voltronic) qpigs: String,
    pub(in crate::voltronic) qmod: String,
    pub(in crate::voltronic) qpiws: String,
    pub(in crate::voltronic) qpiri: String,
    pub(in crate::voltronic) qid: String,
}

use super::impl_seplos_sim_encode::{
    assemble_frame, encode_fault_info, encode_serial_info, encode_telemetry_info,
};
use super::impl_seplos_sim_parse::{classify_malformed_request, parse_request};
use super::prelude::*;

impl SeplosSim {
    /// Answer one request frame per the current [`Behavior`]. Never panics: an unparsable
    /// request (bad framing/checksum) is treated the way a real bus treats noise it can't make
    /// sense of — silence, not a fabricated reply — UNLESS [`SeplosSim::set_auto_nak`] is on and
    /// the request is malformed-but-addressable, in which case it gets a protocol-correct RTN
    /// error frame instead (see `classify_malformed_request`).
    pub fn respond(&mut self, request: &[u8]) -> SimReply {
        if matches!(self.behavior, Behavior::Silence) {
            return SimReply::Silence;
        }
        if matches!(self.behavior, Behavior::Garbage) {
            let mut v: Vec<u8, MAX_FRAME> = Vec::new();
            let _ = v.extend_from_slice(b"not-a-seplos-frame-at-all\r");
            return SimReply::Bytes(v);
        }

        let parsed = match parse_request(request) {
            Some(p) => p,
            None => {
                if self.auto_nak {
                    if let Some((adr, rtn)) = classify_malformed_request(request) {
                        return SimReply::Bytes(assemble_frame(adr, rtn, &[]));
                    }
                }
                return SimReply::Silence;
            }
        };

        match self.behavior {
            Behavior::Normal => SimReply::Bytes(self.build_good_frame(
                parsed.adr,
                parsed.command,
                parsed.command_group,
            )),
            Behavior::RtnError(code) => SimReply::Bytes(assemble_frame(parsed.adr, code, &[])),
            Behavior::CorruptChecksum => {
                let mut frame =
                    self.build_good_frame(parsed.adr, parsed.command, parsed.command_group);
                // Flip a data bit well inside the INFO region so the checksum no longer matches
                // (mirrors lib-io's host_pty.rs FlipChecksumByte fault injection).
                if frame.len() > 20 {
                    frame[20] ^= 0x01;
                }
                SimReply::Bytes(frame)
            }
            Behavior::WrongAddress => {
                // A legitimately well-formed, checksum-VALID frame for a different DIP — a real
                // device answering for someone else, not a corrupted frame.
                let wrong_adr = if parsed.adr == 0x0F {
                    0x00
                } else {
                    parsed.adr.wrapping_add(1)
                };
                SimReply::Bytes(self.build_good_frame(
                    wrong_adr,
                    parsed.command,
                    parsed.command_group,
                ))
            }
            Behavior::Truncated(n) => {
                let mut frame =
                    self.build_good_frame(parsed.adr, parsed.command, parsed.command_group);
                frame.truncate(n.min(frame.len()));
                SimReply::Bytes(frame)
            }
            // Handled above via early return; kept exhaustive (never a panic path).
            Behavior::Silence | Behavior::Garbage => SimReply::Silence,
        }
    }

    /// Build a byte-exact, checksum-valid response frame for `adr`/`command` from the current
    /// [`BatteryState`], echoing `command_group` into the `0x42` reply's command-group byte.
    fn build_good_frame(
        &self,
        adr: u8,
        command: SimCommand,
        command_group: u8,
    ) -> Vec<u8, MAX_FRAME> {
        match command {
            SimCommand::Telemetry => {
                let info = encode_telemetry_info(&self.state, command_group);
                assemble_frame(adr, 0x00, &info)
            }
            SimCommand::Fault => {
                let num_cells = self.state.cell_voltages_mv.len() as u8;
                let info = encode_fault_info(&self.state.faults, num_cells);
                assemble_frame(adr, 0x00, &info)
            }
            SimCommand::SerialNumber => {
                let info = encode_serial_info(&self.state.serial);
                assemble_frame(adr, 0x00, &info)
            }
        }
    }
}

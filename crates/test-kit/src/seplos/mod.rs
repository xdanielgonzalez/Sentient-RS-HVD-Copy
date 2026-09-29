//! `SeplosSim` — a pure, transport-agnostic Seplos v2 BMS responder: request-frame bytes in,
//! response-frame bytes out.
//!
//! Deliberately has **no I/O of its own** — no pty, no std threads, no tokio. [`crate::seplos::SeplosSim::respond`]
//! is a pure function of `(BatteryState, Behavior, request bytes) -> SimReply`, so the exact same
//! core can be driven from a host pty (`test-e2e`'s pipeline e2e), a QEMU UART socket, or real
//! hardware later — the binding layer owns the transport, this module owns the protocol. Uses
//! `heapless`, no `alloc`, byte-layout-compatible with the no_std decoder it mirrors.
//!
//! Frame math (checksum, LENGTH/LCHKSUM) is **reused** from [`lib_parse::seplos::checksum`] —
//! the single authoritative implementation shared with the real decoder — never re-derived here.
//! Only ASCII-hex formatting/parsing (not a checksum) is local to this module.
//!
//! Formerly the standalone `seplos-sim` crate (retired in #99); merged here so all test-support
//! lives in one crate. This module is compiled as part of a std/host test-support crate — the
//! `no_std` posture of the original crate is preserved in spirit (no `alloc`, `heapless` types
//! throughout) even though it is no longer built for the MCU target directly.

mod prelude;
mod types;

mod impl_battery_state;
mod impl_battery_state_spec;
mod impl_drift_spec;
mod impl_seplos_sim;
mod impl_seplos_sim_encode;
mod impl_seplos_sim_parse;
mod impl_seplos_sim_respond;
mod impl_sim_reply;

pub use types::*;

/// Parse the runtime `key=value` [`BatteryState`] spec grammar (see its own doc comment for the
/// full grammar, unit definitions, and fail-closed error cases) — the single parser shared by the
/// `seplos-tcp-sim` stdin control verb, a coming cucumber harness, and Robot Framework keywords.
pub use impl_battery_state_spec::parse_battery_state_spec;

/// Parse the runtime `key=value` [`DriftSpec`] grammar (`"off"`/`"on"`/key=value overlay — see its
/// own doc comment) — the single parser shared by the `seplos-tcp-sim` `drift` stdin control verb
/// and its `--drift` CLI flag.
pub use impl_drift_spec::parse_drift_spec;

#[cfg(test)]
use prelude::*;

/// Max cells represented in one synthesized reading (mirrors
/// [`lib_parse::canonical::MAX_CELLS`]).
pub const MAX_CELLS: usize = 16;
/// Max temperature sensors in one synthesized reading (mirrors
/// [`lib_parse::canonical::MAX_TEMPS`]).
pub const MAX_TEMPS: usize = 8;
/// Max serial-number length synthesized (mirrors
/// [`lib_parse::seplos::serial_number::MAX_SERIAL_LEN`]).
pub const MAX_SERIAL: usize = 32;
/// Max on-wire response frame length (SOI + header + hex INFO + CHKSUM + EOI).
pub const MAX_FRAME: usize = 256;

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use lib_parse::seplos::command::{build_command, Command};
    use lib_parse::seplos::error::SeplosError;
    use lib_parse::seplos::frame::ResponseFrame;
    use lib_parse::seplos::serial_number::SerialNumber;
    use lib_parse::seplos::Seplos;
    use lib_parse::vendor::BmsVendorProtocol;

    fn request(dip: u8, cmd: Command) -> heapless::Vec<u8, 32> {
        let mut out = heapless::Vec::new();
        build_command(dip, cmd, &mut out).expect("build request");
        out
    }

    /// A hex digit guaranteed to differ from `b`, for corrupting exactly one character.
    fn other_hex_digit(b: u8) -> u8 {
        if b == b'0' {
            b'1'
        } else {
            b'0'
        }
    }

    /// Build a request frame with independently-overridable fields, for isolating exactly one
    /// kind of corruption (mirrors `lib-parse`'s `seplos_hardening.rs::make_response` pattern, but
    /// for the *request* side). `info_hex` is raw ASCII-hex characters (not bytes); LENID is
    /// derived from its length. The frame CHKSUM is always freshly computed over whatever bytes
    /// are actually being sent, so the only way to get a bad *frame* checksum is to hand back a
    /// self-consistent frame and then flip one byte afterwards (see call sites below) — this
    /// helper's job is isolating LCHKSUM/CID2 faults, which need independently wrong fields with
    /// an otherwise-correct outer checksum.
    fn make_request(
        adr: u8,
        cid2: &[u8; 2],
        info_hex: &[u8],
        lchksum_override: Option<u8>,
    ) -> std::vec::Vec<u8> {
        use super::impl_seplos_sim_parse::nibble_to_hex;

        let lenid = info_hex.len() as u16;
        let lchksum = lchksum_override.unwrap_or_else(|| length_checksum(lenid));
        let length: u16 = ((lchksum as u16) << 12) | (lenid & 0x0FFF);

        let mut body: std::vec::Vec<u8> = std::vec::Vec::new();
        body.extend_from_slice(b"20"); // VER
        body.push(nibble_to_hex((adr >> 4) & 0xF));
        body.push(nibble_to_hex(adr & 0xF));
        body.extend_from_slice(b"46"); // CID1
        body.extend_from_slice(cid2);
        body.push(nibble_to_hex(((length >> 12) & 0xF) as u8));
        body.push(nibble_to_hex(((length >> 8) & 0xF) as u8));
        body.push(nibble_to_hex(((length >> 4) & 0xF) as u8));
        body.push(nibble_to_hex((length & 0xF) as u8));
        body.extend_from_slice(info_hex);

        let checksum = frame_checksum(&body);
        let mut frame: std::vec::Vec<u8> = std::vec::Vec::new();
        frame.push(b'~');
        frame.extend_from_slice(&body);
        frame.push(nibble_to_hex(((checksum >> 12) & 0xF) as u8));
        frame.push(nibble_to_hex(((checksum >> 8) & 0xF) as u8));
        frame.push(nibble_to_hex(((checksum >> 4) & 0xF) as u8));
        frame.push(nibble_to_hex((checksum & 0xF) as u8));
        frame.push(b'\r');
        frame
    }

    #[test]
    fn round_trip_telemetry_decodes_same_values() {
        let state = BatteryState::sample_14_cell();
        let expected = state.clone();
        let mut sim = SeplosSim::new(state);

        let req = request(0, Command::Telemetry);
        let reply = sim.respond(&req);
        let bytes = match reply {
            SimReply::Bytes(b) => b,
            SimReply::Silence => panic!("expected a reply"),
        };

        let reading = Seplos::new()
            .parse_reading(&bytes, 0)
            .expect("sim-encoded telemetry decodes");
        assert_eq!(reading.device_address, 0);
        assert_eq!(
            reading.cell_voltages_mv.as_slice(),
            expected.cell_voltages_mv.as_slice()
        );
        assert_eq!(
            reading.temperatures_raw.as_slice(),
            expected.temperatures_raw.as_slice()
        );
        assert_eq!(reading.current_ca, expected.current_ca);
        assert_eq!(reading.total_voltage_cv, expected.total_voltage_cv);
        assert_eq!(
            reading.remaining_capacity_cah,
            expected.remaining_capacity_cah
        );
        assert_eq!(reading.typical_capacity_cah, expected.typical_capacity_cah);
        assert_eq!(reading.soc_dpct, expected.soc_dpct);
        assert_eq!(reading.rated_capacity_cah, expected.rated_capacity_cah);
        assert_eq!(reading.cycles, expected.cycles);
        assert_eq!(reading.soh_dpct, expected.soh_dpct);
        assert_eq!(reading.port_voltage_cv, expected.port_voltage_cv);
    }

    #[test]
    fn round_trip_fault_decodes_same_flags() {
        let mut state = BatteryState::sample_14_cell();
        state.faults = FaultFlags {
            high_voltage_protection: true,
            low_temperature_charge_protection: true,
            short_circuit_protection: true,
            charge_current_warning: true,
            ..Default::default()
        };
        let expected_faults = state.faults;
        let mut sim = SeplosSim::new(state);

        let req = request(3, Command::Fault);
        let bytes = match sim.respond(&req) {
            SimReply::Bytes(b) => b,
            SimReply::Silence => panic!("expected a reply"),
        };

        let fault = Seplos::new()
            .parse_fault(&bytes, 3)
            .expect("sim-encoded fault decodes");
        assert_eq!(fault.device_address, 3);
        assert_eq!(
            fault.high_voltage_protection,
            expected_faults.high_voltage_protection
        );
        assert_eq!(
            fault.low_voltage_protection,
            expected_faults.low_voltage_protection
        );
        assert_eq!(
            fault.low_temperature_charge_protection,
            expected_faults.low_temperature_charge_protection
        );
        assert_eq!(
            fault.short_circuit_protection,
            expected_faults.short_circuit_protection
        );
        assert_eq!(
            fault.charge_current_warning,
            expected_faults.charge_current_warning
        );
        assert!(fault.any());
    }

    #[test]
    fn round_trip_all_clear_fault_reports_no_faults() {
        let state = BatteryState::sample_14_cell();
        let mut sim = SeplosSim::new(state);
        let req = request(0, Command::Fault);
        let bytes = match sim.respond(&req) {
            SimReply::Bytes(b) => b,
            SimReply::Silence => panic!("expected a reply"),
        };
        let fault = Seplos::new().parse_fault(&bytes, 0).expect("decodes");
        assert!(!fault.any(), "default FaultFlags must decode as all-clear");
    }

    #[test]
    fn round_trip_serial_number_decodes_same_string() {
        let state = BatteryState::sample_14_cell();
        let expected_serial = state.serial.clone();
        let mut sim = SeplosSim::new(state);
        let req = request(0, Command::SerialNumber);
        let bytes = match sim.respond(&req) {
            SimReply::Bytes(b) => b,
            SimReply::Silence => panic!("expected a reply"),
        };
        let frame = ResponseFrame::parse(&bytes).expect("frame parses");
        let serial = SerialNumber::decode(&frame).expect("serial decodes");
        assert_eq!(serial.0.as_str(), expected_serial.as_str());
    }

    #[test]
    fn behavior_silence_returns_no_bytes() {
        let mut sim = SeplosSim::new(BatteryState::sample_14_cell());
        sim.set_behavior(Behavior::Silence);
        let req = request(0, Command::Telemetry);
        assert_eq!(sim.respond(&req), SimReply::Silence);
    }

    #[test]
    fn behavior_corrupt_checksum_fails_the_real_decoder() {
        let mut sim = SeplosSim::new(BatteryState::sample_14_cell());
        sim.set_behavior(Behavior::CorruptChecksum);
        let req = request(0, Command::Telemetry);
        let bytes = match sim.respond(&req) {
            SimReply::Bytes(b) => b,
            SimReply::Silence => panic!("expected a reply"),
        };
        assert!(
            Seplos::new().parse_reading(&bytes, 0).is_err(),
            "a corrupted checksum must never be accepted as valid"
        );
    }

    #[test]
    fn behavior_wrong_address_is_rejected_for_the_polled_dip() {
        let mut sim = SeplosSim::new(BatteryState::sample_14_cell());
        sim.set_behavior(Behavior::WrongAddress);
        let req = request(0, Command::Telemetry);
        let bytes = match sim.respond(&req) {
            SimReply::Bytes(b) => b,
            SimReply::Silence => panic!("expected a reply"),
        };
        // The frame itself is well-formed and checksum-valid (just for the wrong DIP), proving
        // this exercises AddressMismatch specifically, not an incidental checksum failure.
        let frame = ResponseFrame::parse(&bytes).expect("well-formed frame");
        assert_ne!(
            frame.adr, 0,
            "must be addressed to a DIFFERENT dip than polled"
        );
        assert!(
            Seplos::new().parse_reading(&bytes, 0).is_err(),
            "a reply for a different DIP must never be accepted for DIP 0"
        );
    }

    #[test]
    fn behavior_truncated_never_reaches_the_delimiter() {
        let mut sim = SeplosSim::new(BatteryState::sample_14_cell());
        sim.set_behavior(Behavior::Truncated(10));
        let req = request(0, Command::Telemetry);
        let bytes = match sim.respond(&req) {
            SimReply::Bytes(b) => b,
            SimReply::Silence => panic!("expected a reply"),
        };
        assert_eq!(bytes.len(), 10);
        assert_ne!(
            bytes.last(),
            Some(&b'\r'),
            "truncated frame must not carry the delimiter"
        );
    }

    #[test]
    fn behavior_garbage_is_error_not_panic() {
        let mut sim = SeplosSim::new(BatteryState::sample_14_cell());
        sim.set_behavior(Behavior::Garbage);
        let req = request(0, Command::Telemetry);
        let bytes = match sim.respond(&req) {
            SimReply::Bytes(b) => b,
            SimReply::Silence => panic!("expected a reply"),
        };
        assert!(
            bytes.last() == Some(&b'\r'),
            "garbage still ends in the delimiter"
        );
        assert!(ResponseFrame::parse(&bytes).is_err());
    }

    #[test]
    fn malformed_request_gets_silence_not_a_fabricated_reply() {
        let mut sim = SeplosSim::new(BatteryState::sample_14_cell());
        let mut bad_req = request(0, Command::Telemetry);
        // Corrupt the request's own checksum.
        let n = bad_req.len();
        bad_req[n - 5] ^= 0x01;
        assert_eq!(sim.respond(&bad_req), SimReply::Silence);
    }

    #[test]
    fn as_bytes_matches_variant() {
        assert_eq!(SimReply::Silence.as_bytes(), &[] as &[u8]);
        let mut v: Vec<u8, MAX_FRAME> = Vec::new();
        let _ = v.push(b'x');
        assert_eq!(SimReply::Bytes(v).as_bytes(), b"x");
    }

    #[test]
    fn telemetry_reply_is_160_bytes_and_decodes_unchanged() {
        // The real Seplos 48150-1101-10E's 14-cell/6-temp reply is 160 bytes on the wire (a
        // 71-byte INFO); the sim previously stopped at 63 bytes (144 on the wire), missing the
        // trailing reserved block a real decoder never reads but a byte-length-sensitive
        // downstream (fixed-size buffers, framing timeouts) can still care about.
        let state = BatteryState::sample_14_cell();
        let expected = state.clone();
        let mut sim = SeplosSim::new(state);

        let req = request(0, Command::Telemetry);
        let bytes = match sim.respond(&req) {
            SimReply::Bytes(b) => b,
            SimReply::Silence => panic!("expected a reply"),
        };
        assert_eq!(
            bytes.len(),
            160,
            "sim telemetry reply must match the real device's 160-byte wire frame"
        );

        let reading = Seplos::new()
            .parse_reading(&bytes, 0)
            .expect("sim-encoded telemetry decodes");
        assert_eq!(
            reading.cell_voltages_mv.as_slice(),
            expected.cell_voltages_mv.as_slice()
        );
        assert_eq!(
            reading.temperatures_raw.as_slice(),
            expected.temperatures_raw.as_slice()
        );
        assert_eq!(reading.current_ca, expected.current_ca);
        assert_eq!(reading.total_voltage_cv, expected.total_voltage_cv);
        assert_eq!(
            reading.remaining_capacity_cah,
            expected.remaining_capacity_cah
        );
        assert_eq!(reading.typical_capacity_cah, expected.typical_capacity_cah);
        assert_eq!(reading.soc_dpct, expected.soc_dpct);
        assert_eq!(reading.rated_capacity_cah, expected.rated_capacity_cah);
        assert_eq!(reading.cycles, expected.cycles);
        assert_eq!(reading.soh_dpct, expected.soh_dpct);
        assert_eq!(reading.port_voltage_cv, expected.port_voltage_cv);
    }

    #[test]
    fn accepts_the_repo_builder_1_nibble_request_variant() {
        // LENID=001, a single hex-char INFO (the DIP nibble) — `build_command`'s own shape.
        let req = make_request(0, b"42", b"0", None);
        let mut sim = SeplosSim::new(BatteryState::sample_14_cell());
        let bytes = match sim.respond(&req) {
            SimReply::Bytes(b) => b,
            SimReply::Silence => panic!("the 1-nibble request variant must be accepted"),
        };
        assert!(ResponseFrame::parse(&bytes).is_ok());
    }

    #[test]
    fn accepts_the_documented_2_hex_char_command_group_request_variant() {
        // LENID=002, a 2-hex-char INFO byte holding the command-group/pack selector — the
        // documented protocol V2.0 / syssi `esphome-seplos-bms` real-TX shape
        // (`~20004642E00200FD37`, group byte 0x00).
        let req = make_request(0, b"42", b"00", None);
        assert_eq!(&req[..], b"~20004642E00200FD37\r");

        let mut sim = SeplosSim::new(BatteryState::sample_14_cell());
        let bytes = match sim.respond(&req) {
            SimReply::Bytes(b) => b,
            SimReply::Silence => {
                panic!("the 2-hex-char command-group request variant must be accepted")
            }
        };
        let reading = Seplos::new()
            .parse_reading(&bytes, 0)
            .expect("sim-encoded telemetry decodes");
        assert_eq!(reading.device_address, 0);
    }

    #[test]
    fn command_group_request_variant_echoes_the_group_into_the_reply() {
        // A non-zero group byte (0x01, matching syssi's real RX example) must come back out in
        // the telemetry reply's command-group byte (INFO byte 1).
        let req = make_request(0, b"42", b"01", None);
        let mut sim = SeplosSim::new(BatteryState::sample_14_cell());
        let bytes = match sim.respond(&req) {
            SimReply::Bytes(b) => b,
            SimReply::Silence => panic!("expected a reply"),
        };
        let frame = ResponseFrame::parse(&bytes).expect("frame parses");
        assert_eq!(
            frame.info[1], 0x01,
            "command-group byte must echo the request's group selector"
        );
    }

    #[test]
    fn behavior_rtn_error_forces_the_named_code() {
        let mut sim = SeplosSim::new(BatteryState::sample_14_cell());
        sim.set_behavior(Behavior::RtnError(0x04));
        let req = request(0, Command::Telemetry);
        let bytes = match sim.respond(&req) {
            SimReply::Bytes(b) => b,
            SimReply::Silence => panic!("expected a reply"),
        };
        // Still a well-formed, checksum-valid frame — just RTN != 0x00.
        assert_eq!(
            ResponseFrame::parse(&bytes),
            Err(SeplosError::RtnError(0x04))
        );
    }

    #[test]
    fn auto_nak_off_by_default_malformed_request_still_gets_silence() {
        // Auto-NAK defaults to off, so every pre-existing "malformed -> Silence" test (including
        // `malformed_request_gets_silence_not_a_fabricated_reply` above) keeps passing unchanged.
        let mut sim = SeplosSim::new(BatteryState::sample_14_cell());
        let good = make_request(0, b"42", b"0", None);
        let mut bad_checksum = good.clone();
        let n = bad_checksum.len();
        bad_checksum[n - 6] = other_hex_digit(bad_checksum[n - 6]); // corrupt the INFO char
        assert_eq!(sim.respond(&bad_checksum), SimReply::Silence);
    }

    #[test]
    fn auto_nak_bad_frame_checksum_gets_rtn_02() {
        let mut sim = SeplosSim::new(BatteryState::sample_14_cell());
        sim.set_auto_nak(true);
        let mut req = make_request(0, b"42", b"0", None);
        // Corrupt the INFO character (leaves LENGTH/LCHKSUM untouched, breaks only the outer
        // frame checksum).
        let n = req.len();
        req[n - 6] = other_hex_digit(req[n - 6]);
        let bytes = match sim.respond(&req) {
            SimReply::Bytes(b) => b,
            SimReply::Silence => panic!("auto-NAK must reply with an RTN error frame"),
        };
        assert_eq!(
            ResponseFrame::parse(&bytes),
            Err(SeplosError::RtnError(0x02))
        );
    }

    #[test]
    fn auto_nak_bad_length_checksum_gets_rtn_03() {
        let mut sim = SeplosSim::new(BatteryState::sample_14_cell());
        sim.set_auto_nak(true);
        // A wrong LCHKSUM nibble, with the frame CHKSUM computed fresh over the (self-consistent,
        // if logically-wrong) bytes actually sent — isolates the LCHKSUM fault specifically.
        let bad_lchksum = (length_checksum(1) ^ 0x1) & 0xF;
        let req = make_request(0, b"42", b"0", Some(bad_lchksum));
        let bytes = match sim.respond(&req) {
            SimReply::Bytes(b) => b,
            SimReply::Silence => panic!("auto-NAK must reply with an RTN error frame"),
        };
        assert_eq!(
            ResponseFrame::parse(&bytes),
            Err(SeplosError::RtnError(0x03))
        );
    }

    #[test]
    fn auto_nak_invalid_cid2_gets_rtn_04() {
        let mut sim = SeplosSim::new(BatteryState::sample_14_cell());
        sim.set_auto_nak(true);
        // "99" is not a CID2 the sim recognizes (0x42/0x44/0xA4); checksum/LCHKSUM are otherwise
        // self-consistent since `make_request` computes them fresh.
        let req = make_request(0, b"99", b"0", None);
        let bytes = match sim.respond(&req) {
            SimReply::Bytes(b) => b,
            SimReply::Silence => panic!("auto-NAK must reply with an RTN error frame"),
        };
        assert_eq!(
            ResponseFrame::parse(&bytes),
            Err(SeplosError::RtnError(0x04))
        );
    }
}

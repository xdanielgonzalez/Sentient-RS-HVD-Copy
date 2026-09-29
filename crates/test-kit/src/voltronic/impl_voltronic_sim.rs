use lib_parse::voltronic::crc::crc16_xmodem;

use super::types::VoltronicSim;

/// A default realistic `QPIGS` status payload (21 space-separated fields; PROVISIONAL). Decodes to:
/// grid 230.4 V / 50.0 Hz, AC output 230.0 V / 50.0 Hz, 900 VA / 850 W, bus 400 V, battery 51.20 V,
/// 8 A charge / 0 A discharge, 27 °C, PV 5.0 A / 380.0 V / 1900 W, status bits load_on+charging+
/// solar_charging (`00010110`).
const DEFAULT_QPIGS: &str =
    "230.4 50.0 230.0 50.0 0900 0850 018 400 51.20 008 085 0027 5.0 380.0 057 00000 00010110 00 00 01900 010";
/// Default `QMOD` = Line mode (`L`).
const DEFAULT_QMOD: &str = "L";
/// Default `QPIRI` ratings (25 fields): field 7 variant `5000`, field 13 battery type `3` (lithium),
/// field 22 output mode `00` (single phase).
const DEFAULT_QPIRI: &str =
    "230.0 21.7 230.0 50.0 21.7 5000 5000 48.0 46.0 42.0 54.0 56.4 3 10 060 0 2 00 0 01 0 00 0 0 27";
/// Default `QID` serial number.
const DEFAULT_QID: &str = "96332004100443";

/// Build a `QPIWS` fault payload (36 binary digits) with `1` at each index in `set`, `0` elsewhere.
/// `bit index = string position`, per the Voltronic QPIWS convention (line_fail=5, overtemperature=9,
/// fan_locked=10, battery_low=12, overload=15, output_short=17).
fn qpiws_bits(set: &[usize]) -> String {
    (0..36)
        .map(|i| if set.contains(&i) { '1' } else { '0' })
        .collect()
}

/// Frame a response payload the way a real Voltronic device does: `(` + payload + CRC-16/XMODEM
/// (2 bytes, big-endian) + `\r`, the CRC computed over `(` + payload (everything before the CRC),
/// matching `lib_parse::voltronic::Voltronic::validate_frame` exactly.
fn frame(payload: &str) -> Vec<u8> {
    let mut f = Vec::with_capacity(payload.len() + 4);
    f.push(b'(');
    f.extend_from_slice(payload.as_bytes());
    let crc = crc16_xmodem(&f);
    f.push((crc >> 8) as u8);
    f.push((crc & 0xFF) as u8);
    f.push(b'\r');
    f
}

impl VoltronicSim {
    /// Construct a responder with realistic default payloads (all clear, Line mode).
    pub fn new() -> Self {
        VoltronicSim {
            qpigs: DEFAULT_QPIGS.to_string(),
            qmod: DEFAULT_QMOD.to_string(),
            qpiws: qpiws_bits(&[]),
            qpiri: DEFAULT_QPIRI.to_string(),
            qid: DEFAULT_QID.to_string(),
        }
    }

    /// Set the `QMOD` operating-mode character (`P|S|L|B|F|H`).
    pub fn set_mode(&mut self, mode: char) {
        self.qmod = mode.to_string();
    }

    /// Replace the `QPIWS` fault payload verbatim (36-ish binary digits).
    pub fn set_qpiws(&mut self, payload: &str) {
        self.qpiws = payload.to_string();
    }

    /// Convenience fault-injection knob: set the `QPIWS` overload bit (index 15).
    pub fn set_overload_fault(&mut self) {
        self.qpiws = qpiws_bits(&[15]);
    }

    /// Answer one request frame: identify the command by its leading ASCII (robust to where the
    /// request's own CRC/`\r` land) and return the canned response frame, or `None` for an
    /// unrecognized command (a real device would stay silent).
    pub fn respond(&self, request: &[u8]) -> Option<Vec<u8>> {
        let payload = if request.starts_with(b"QPIGS") {
            &self.qpigs
        } else if request.starts_with(b"QPIWS") {
            &self.qpiws
        } else if request.starts_with(b"QPIRI") {
            &self.qpiri
        } else if request.starts_with(b"QMOD") {
            &self.qmod
        } else if request.starts_with(b"QID") {
            &self.qid
        } else {
            return None;
        };
        Some(frame(payload))
    }
}

impl Default for VoltronicSim {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use lib_parse::vendor::{InverterCommand, InverterVendorProtocol};
    use lib_parse::voltronic::Voltronic;

    /// Build the driver's request frame for a command (command bytes + CRC + `\r`) so the sim's
    /// prefix-match is exercised exactly as the real driver would drive it.
    fn request(command: InverterCommand) -> heapless::Vec<u8, 16> {
        let mut out = heapless::Vec::new();
        Voltronic::new()
            .build_request(command, &mut out)
            .expect("build request");
        out
    }

    #[test]
    fn qpigs_frame_validates_and_decodes_expected_values() {
        let sim = VoltronicSim::new();
        let bytes = sim
            .respond(&request(InverterCommand::Status))
            .expect("reply");
        let v = Voltronic::new();
        assert!(
            v.validate_frame(&bytes),
            "sim QPIGS frame must pass the real CRC check"
        );
        let r = v.parse_status(&bytes).expect("decodes");
        assert_eq!(r.grid_voltage_dv, 2304);
        assert_eq!(r.ac_output_apparent_power_va, 900);
        assert_eq!(r.ac_output_active_power_w, 850);
        assert_eq!(r.battery_voltage_cv, 5120);
        assert_eq!(r.pv_voltage_dv, 3800);
        assert_eq!(r.inverter_temperature_c, 27);
        assert!(r.load_on());
        assert!(r.charging());
        assert!(r.solar_charging());
        assert!(!r.ac_charging());
    }

    #[test]
    fn qmod_defaults_to_line_and_is_settable() {
        let v = Voltronic::new();
        let mut sim = VoltronicSim::new();
        let bytes = sim.respond(&request(InverterCommand::Mode)).expect("reply");
        assert_eq!(
            v.parse_mode(&bytes).expect("decodes"),
            lib_parse::canonical::InverterMode::Line
        );
        sim.set_mode('F');
        let bytes = sim.respond(&request(InverterCommand::Mode)).expect("reply");
        assert_eq!(
            v.parse_mode(&bytes).expect("decodes"),
            lib_parse::canonical::InverterMode::Fault
        );
    }

    #[test]
    fn qpiws_overload_bit_round_trips() {
        let v = Voltronic::new();
        let mut sim = VoltronicSim::new();
        // All-clear by default.
        let clear = v
            .parse_fault(
                &sim.respond(&request(InverterCommand::Fault))
                    .expect("reply"),
            )
            .expect("decodes");
        assert!(!clear.overload_fault);
        sim.set_overload_fault();
        let faulted = v
            .parse_fault(
                &sim.respond(&request(InverterCommand::Fault))
                    .expect("reply"),
            )
            .expect("decodes");
        assert!(faulted.overload_fault);
        assert!(!faulted.line_fail);
    }

    #[test]
    fn qpiri_and_qid_decode() {
        let v = Voltronic::new();
        let sim = VoltronicSim::new();
        let ratings = v
            .parse_ratings(
                &sim.respond(&request(InverterCommand::Ratings))
                    .expect("reply"),
            )
            .expect("decodes");
        assert_eq!(ratings.battery_type.as_str(), "lithium");
        assert_eq!(ratings.output_mode.as_str(), "single_phase");
        assert_eq!(ratings.variant.as_str(), "5000");

        let mut serial = heapless::String::<32>::new();
        v.parse_serial(
            &sim.respond(&request(InverterCommand::SerialNumber))
                .expect("reply"),
            &mut serial,
        )
        .expect("decodes");
        assert_eq!(serial.as_str(), "96332004100443");
    }

    #[test]
    fn unrecognized_command_is_silence() {
        let sim = VoltronicSim::new();
        assert!(sim.respond(b"QBOGUS\r").is_none());
    }
}

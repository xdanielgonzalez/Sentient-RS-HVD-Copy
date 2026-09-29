//! Fuzz the Seplos command encode/decode path.
//!
//! Encode: `seplos::command::build_command(dip, Command, &mut heapless::Vec<u8, 32>)`, the
//! single authoritative frame builder. Decode: feed the encoder's own output (and the raw fuzz
//! bytes directly) back through `seplos::frame::ResponseFrame::parse`, since a Seplos request and
//! response share on-wire shape (`~ VER ADR CID1 CID2/RTN LENGTH INFO CHKSUM \r`) — this exercises
//! the parser on attacker-controlled *and* well-formed-encoder-shaped input without ever panicking.
#![no_main]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use libfuzzer_sys::fuzz_target;
use lib_parse::seplos::{
    command::{build_command, Command},
    frame::ResponseFrame,
};

fuzz_target!(|data: &[u8]| {
    // Direct decode fuzzing on raw untrusted bytes.
    let _ = ResponseFrame::parse(data);

    if data.is_empty() {
        return;
    }
    let dip = data[0];
    let command = match data.get(1).copied().unwrap_or(0) % 3 {
        0 => Command::Telemetry,
        1 => Command::Fault,
        _ => Command::SerialNumber,
    };

    let mut out: heapless::Vec<u8, 32> = heapless::Vec::new();
    if build_command(dip, command, &mut out).is_ok() {
        // Round-trip the encoder's own output through the decoder.
        let _ = ResponseFrame::parse(&out);
    }
});

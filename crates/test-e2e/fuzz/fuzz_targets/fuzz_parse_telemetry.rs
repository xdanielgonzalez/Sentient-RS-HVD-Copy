//! Fuzz the Seplos `0x42` telemetry decode path with raw untrusted RS-485 bytes.
//!
//! Real entry points (see `lib-parse/src/seplos/{frame,telemetry}.rs`):
//! `seplos::frame::ResponseFrame::parse(&[u8]) -> Result<ResponseFrame, SeplosError>`, then
//! `seplos::telemetry::decode_reading(&ResponseFrame) -> Result<CanonicalReading, SeplosError>`.
//! Both are documented "never panics" — this target is the executable proof of that claim.
#![no_main]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use libfuzzer_sys::fuzz_target;
use lib_parse::seplos::{frame::ResponseFrame, telemetry};

fuzz_target!(|data: &[u8]| {
    if let Ok(frame) = ResponseFrame::parse(data) {
        let _ = telemetry::decode_reading(&frame);
    }
});

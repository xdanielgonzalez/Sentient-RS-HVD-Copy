//! Fuzz the canonical → TMS mapping round-trip, driven off raw bytes through the *real* parse
//! chain (never hand-built `CanonicalReading`/`CanonicalFault` values, which would fuzz a fake
//! surface): `seplos::frame::ResponseFrame::parse` → `seplos::telemetry::decode_reading` /
//! `seplos::fault::decode_fault` → `lib_mapping::transform::{ToTms, ToDiagnostics}` →
//! `lib_mapping::codec::to_json_slice`. Must not panic anywhere in the chain, and the
//! no_std JSON codec must never overrun a bounded buffer.
#![no_main]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use heapless::String;
use libfuzzer_sys::fuzz_target;
use lib_mapping::{
    codec::to_json_slice,
    schema::ClockMonotonic,
    transform::{ToDiagnostics, ToTms},
};
use lib_parse::seplos::{fault, frame::ResponseFrame, telemetry};

fuzz_target!(|data: &[u8]| {
    let Ok(frame) = ResponseFrame::parse(data) else {
        return;
    };

    let now = ClockMonotonic {
        seconds: 0,
        nanoseconds: 0,
    };
    let mut device_id: String<32> = String::new();
    let _ = device_id.push_str("fuzz-device");

    if let Ok(reading) = telemetry::decode_reading(&frame) {
        let dc = reading.to_dc_measurement(device_id.clone(), now);
        let hw = reading.to_power_hardware(device_id.clone(), now);

        let mut buf = [0u8; 4096];
        let _ = to_json_slice(&dc, &mut buf);
        let _ = to_json_slice(&hw, &mut buf);
    }

    if let Ok(f) = fault::decode_fault(&frame) {
        let diag = f.to_active_diagnostics(device_id, now);
        let mut buf = [0u8; 4096];
        let _ = to_json_slice(&diag, &mut buf);
    }
});

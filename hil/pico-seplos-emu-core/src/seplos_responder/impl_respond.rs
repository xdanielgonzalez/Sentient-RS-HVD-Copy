use super::impl_encode::{assemble_frame, encode_telemetry_info};
use super::impl_parse::parse_request;
use super::prelude::*;

impl SeplosResponder {
    /// Answer one request frame. Never panics: an unparsable or non-telemetry request is
    /// treated the way a real bus treats noise/commands it doesn't implement — silence, never
    /// a fabricated reply.
    pub fn respond(&mut self, request: &[u8]) -> SimReply {
        let adr = match parse_request(request) {
            Some(adr) => adr,
            None => return SimReply::Silence,
        };
        let info = encode_telemetry_info(&self.state, 0);
        SimReply::Bytes(assemble_frame(adr, 0x00, &info))
    }
}

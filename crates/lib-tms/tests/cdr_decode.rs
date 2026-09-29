//! XCDR2 encode↔decode round-trips for the two inbound TMS request types, anchored to LITERAL
//! goldens computed by hand from the DDS-XTypes 1.3 rules (see `lib_tms::cdr`'s module doc for the
//! per-rule cites). The encode side drives WP2's primitives through hand-written CdrField chains
//! (what the WP3 codegen will emit); the goldens are an independent oracle, so encode and decode
//! cannot share a symmetric bug.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::all)]

use lib_tms::cdr::decode::{decode_energy_start_stop_request, decode_power_switch_request};
use lib_tms::cdr::{
    emit_dheader, emit_object_cdr, emit_optional, CdrChain, CdrCountingSink, CdrError, CdrField,
    CdrNil, CdrNode, CdrSink, CdrWriteSink,
};
use lib_tms::generated as g;

fn s32(s: &str) -> heapless::String<32> {
    heapless::String::try_from(s).unwrap()
}

// ------------------------------------------------------------------------------------------------
// Hand-written CdrField chains (WP3 will generate these) for the two request types + nested types.
// ------------------------------------------------------------------------------------------------

// --- DeviceConfigRequest ---
struct DcrReqDev;
impl CdrField<g::DeviceConfigRequest> for DcrReqDev {
    const ORDER: u16 = 0;
    fn emit<S: CdrSink>(src: &g::DeviceConfigRequest, sink: &mut S) {
        sink.string(&src.requesting_device_id);
    }
}
struct DcrTgtDev;
impl CdrField<g::DeviceConfigRequest> for DcrTgtDev {
    const ORDER: u16 = 1;
    fn emit<S: CdrSink>(src: &g::DeviceConfigRequest, sink: &mut S) {
        sink.string(&src.target_device_id);
    }
}
struct DcrConfig;
impl CdrField<g::DeviceConfigRequest> for DcrConfig {
    const ORDER: u16 = 2;
    fn emit<S: CdrSink>(src: &g::DeviceConfigRequest, sink: &mut S) {
        sink.enum_u32(src.config as u32);
    }
}
type DcrChain = CdrNode<DcrReqDev, CdrNode<DcrTgtDev, CdrNode<DcrConfig, CdrNil>>>;

// --- PowerPortConfigRequest ---
struct PpcrReqDev;
impl CdrField<g::PowerPortConfigRequest> for PpcrReqDev {
    const ORDER: u16 = 0;
    fn emit<S: CdrSink>(src: &g::PowerPortConfigRequest, sink: &mut S) {
        sink.string(&src.requesting_device_id);
    }
}
struct PpcrTgtDev;
impl CdrField<g::PowerPortConfigRequest> for PpcrTgtDev {
    const ORDER: u16 = 1;
    fn emit<S: CdrSink>(src: &g::PowerPortConfigRequest, sink: &mut S) {
        sink.string(&src.target_device_id);
    }
}
struct PpcrConfig;
impl CdrField<g::PowerPortConfigRequest> for PpcrConfig {
    const ORDER: u16 = 2;
    fn emit<S: CdrSink>(src: &g::PowerPortConfigRequest, sink: &mut S) {
        sink.enum_u32(src.config as u32);
    }
}
struct PpcrPort;
impl CdrField<g::PowerPortConfigRequest> for PpcrPort {
    const ORDER: u16 = 3;
    fn emit<S: CdrSink>(src: &g::PowerPortConfigRequest, sink: &mut S) {
        sink.i32(src.port_number);
    }
}
type PpcrChain =
    CdrNode<PpcrReqDev, CdrNode<PpcrTgtDev, CdrNode<PpcrConfig, CdrNode<PpcrPort, CdrNil>>>>;

// --- PowerSwitchCloseConditions ---
struct PsccMinV;
impl CdrField<g::PowerSwitchCloseConditions> for PsccMinV {
    const ORDER: u16 = 0;
    fn emit<S: CdrSink>(src: &g::PowerSwitchCloseConditions, sink: &mut S) {
        sink.f32(src.min_voltage);
    }
}
struct PsccMaxV;
impl CdrField<g::PowerSwitchCloseConditions> for PsccMaxV {
    const ORDER: u16 = 1;
    fn emit<S: CdrSink>(src: &g::PowerSwitchCloseConditions, sink: &mut S) {
        sink.f32(src.max_voltage);
    }
}
struct PsccMinF;
impl CdrField<g::PowerSwitchCloseConditions> for PsccMinF {
    const ORDER: u16 = 2;
    fn emit<S: CdrSink>(src: &g::PowerSwitchCloseConditions, sink: &mut S) {
        sink.f32(src.min_frequency);
    }
}
struct PsccMaxF;
impl CdrField<g::PowerSwitchCloseConditions> for PsccMaxF {
    const ORDER: u16 = 3;
    fn emit<S: CdrSink>(src: &g::PowerSwitchCloseConditions, sink: &mut S) {
        sink.f32(src.max_frequency);
    }
}
type PsccChain = CdrNode<PsccMinV, CdrNode<PsccMaxV, CdrNode<PsccMinF, CdrNode<PsccMaxF, CdrNil>>>>;

fn emit_switch_conditions<S: CdrSink>(v: &g::PowerSwitchCloseConditions, sink: &mut S) {
    emit_dheader::<g::PowerSwitchCloseConditions, PsccChain, S>(v, sink);
}

// --- EnergyStartStopRequest ---
struct EssReqId;
impl CdrField<g::EnergyStartStopRequest> for EssReqId {
    const ORDER: u16 = 0;
    fn emit<S: CdrSink>(src: &g::EnergyStartStopRequest, sink: &mut S) {
        emit_dheader::<g::DeviceConfigRequest, DcrChain, S>(&src.request_id, sink);
    }
}
struct EssSeq;
impl CdrField<g::EnergyStartStopRequest> for EssSeq {
    const ORDER: u16 = 1;
    fn emit<S: CdrSink>(src: &g::EnergyStartStopRequest, sink: &mut S) {
        sink.u64(src.sequence_id);
    }
}
struct EssFrom;
impl CdrField<g::EnergyStartStopRequest> for EssFrom {
    const ORDER: u16 = 2;
    fn emit<S: CdrSink>(src: &g::EnergyStartStopRequest, sink: &mut S) {
        sink.enum_u32(src.from_level as u32);
    }
}
struct EssTo;
impl CdrField<g::EnergyStartStopRequest> for EssTo {
    const ORDER: u16 = 3;
    fn emit<S: CdrSink>(src: &g::EnergyStartStopRequest, sink: &mut S) {
        sink.enum_u32(src.to_level as u32);
    }
}
struct EssSwitch;
impl CdrField<g::EnergyStartStopRequest> for EssSwitch {
    const ORDER: u16 = 4;
    fn emit<S: CdrSink>(src: &g::EnergyStartStopRequest, sink: &mut S) {
        emit_optional(
            sink,
            src.switch_conditions.as_ref(),
            emit_switch_conditions::<S>,
        );
    }
}
type EssChain = CdrNode<
    EssReqId,
    CdrNode<EssSeq, CdrNode<EssFrom, CdrNode<EssTo, CdrNode<EssSwitch, CdrNil>>>>,
>;

// --- PowerSwitchRequest ---
struct PsrReqId;
impl CdrField<g::PowerSwitchRequest> for PsrReqId {
    const ORDER: u16 = 0;
    fn emit<S: CdrSink>(src: &g::PowerSwitchRequest, sink: &mut S) {
        emit_dheader::<g::PowerPortConfigRequest, PpcrChain, S>(&src.request_id, sink);
    }
}
struct PsrSeq;
impl CdrField<g::PowerSwitchRequest> for PsrSeq {
    const ORDER: u16 = 1;
    fn emit<S: CdrSink>(src: &g::PowerSwitchRequest, sink: &mut S) {
        sink.u64(src.sequence_id);
    }
}
struct PsrCont;
impl CdrField<g::PowerSwitchRequest> for PsrCont {
    const ORDER: u16 = 2;
    fn emit<S: CdrSink>(src: &g::PowerSwitchRequest, sink: &mut S) {
        sink.enum_u32(src.continuity as u32);
    }
}
struct PsrSwitch;
impl CdrField<g::PowerSwitchRequest> for PsrSwitch {
    const ORDER: u16 = 3;
    fn emit<S: CdrSink>(src: &g::PowerSwitchRequest, sink: &mut S) {
        emit_optional(
            sink,
            src.switch_conditions.as_ref(),
            emit_switch_conditions::<S>,
        );
    }
}
type PsrChain = CdrNode<PsrReqId, CdrNode<PsrSeq, CdrNode<PsrCont, CdrNode<PsrSwitch, CdrNil>>>>;

// ------------------------------------------------------------------------------------------------
// Encode helper
// ------------------------------------------------------------------------------------------------

fn encode<Src, C: CdrChain<Src>>(src: &Src) -> Vec<u8> {
    let mut buf = [0u8; 256];
    let mut sink = CdrWriteSink::new(&mut buf);
    emit_object_cdr::<Src, C, _>(src, &mut sink);
    sink.finalize_encapsulation_padding();
    assert!(!sink.overflowed());
    let n = sink.len();
    buf[..n].to_vec()
}

// ------------------------------------------------------------------------------------------------
// EnergyStartStopRequest — switch_conditions ABSENT
// ------------------------------------------------------------------------------------------------

#[rustfmt::skip]
const ESS_GOLDEN: [u8; 52] = [
    0x00, 0x09, 0x00, 0x03,                         // encap D_CDR2_LE + options (3 trailing pad)
    0x29, 0x00, 0x00, 0x00,                         // top DHEADER = 41
    0x14, 0x00, 0x00, 0x00,                         // request_id (DeviceConfigRequest) DHEADER = 20
    0x04, 0x00, 0x00, 0x00, 0x72, 0x65, 0x71, 0x00, // "req\0"
    0x04, 0x00, 0x00, 0x00, 0x74, 0x67, 0x74, 0x00, // "tgt\0"
    0x02, 0x00, 0x00, 0x00,                         // config = Active(2)
    0x07, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // sequence_id = 7 (u64, align 4)
    0x02, 0x00, 0x00, 0x00,                         // from_level = Off(2)
    0x07, 0x00, 0x00, 0x00,                         // to_level = Operational(7)
    0x00,                                           // switch_conditions absent (is_present=false)
    0x00, 0x00, 0x00,                               // trailing padding to 4-byte boundary
];

fn ess_sample() -> g::EnergyStartStopRequest {
    g::EnergyStartStopRequest {
        request_id: g::DeviceConfigRequest {
            requesting_device_id: s32("req"),
            target_device_id: s32("tgt"),
            config: g::ConfigId::Active,
        },
        sequence_id: 7,
        from_level: g::EnergyStartStopLevel::Off,
        to_level: g::EnergyStartStopLevel::Operational,
        switch_conditions: None,
    }
}

#[test]
fn ess_encode_matches_golden() {
    let bytes = encode::<g::EnergyStartStopRequest, EssChain>(&ess_sample());
    assert_eq!(bytes.as_slice(), &ESS_GOLDEN[..]);
}

#[test]
fn ess_decode_golden_matches_sample() {
    let decoded = decode_energy_start_stop_request(&ESS_GOLDEN).unwrap();
    assert_eq!(decoded, ess_sample());
}

#[test]
fn ess_round_trip() {
    let sample = ess_sample();
    let bytes = encode::<g::EnergyStartStopRequest, EssChain>(&sample);
    assert_eq!(decode_energy_start_stop_request(&bytes).unwrap(), sample);
}

// ------------------------------------------------------------------------------------------------
// PowerSwitchRequest — switch_conditions PRESENT (exercises optional-present + nested f32 struct)
// ------------------------------------------------------------------------------------------------

#[rustfmt::skip]
const PSR_GOLDEN: [u8; 72] = [
    0x00, 0x09, 0x00, 0x00,                         // encap D_CDR2_LE + options (no trailing pad)
    0x40, 0x00, 0x00, 0x00,                         // top DHEADER = 64
    0x18, 0x00, 0x00, 0x00,                         // request_id (PowerPortConfigRequest) DHEADER=24
    0x02, 0x00, 0x00, 0x00, 0x61, 0x00,             // "a\0"
    0x00, 0x00,                                     // pad to align next string length to 4
    0x03, 0x00, 0x00, 0x00, 0x62, 0x62, 0x00,       // "bb\0"
    0x00,                                           // pad to align enum to 4
    0x01, 0x00, 0x00, 0x00,                         // config = Defaults(1)
    0x05, 0x00, 0x00, 0x00,                         // port_number = 5 (i32)
    0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // sequence_id = 1 (u64, align 4)
    0x02, 0x00, 0x00, 0x00,                         // continuity = Closed(2)
    0x01,                                           // switch_conditions present (is_present=true)
    0x00, 0x00, 0x00,                               // pad to align nested DHEADER to 4
    0x10, 0x00, 0x00, 0x00,                         // PowerSwitchCloseConditions DHEADER = 16
    0x00, 0x00, 0x80, 0x3F,                         // min_voltage = 1.0
    0x00, 0x00, 0x00, 0x40,                         // max_voltage = 2.0
    0x00, 0x00, 0x40, 0x40,                         // min_frequency = 3.0
    0x00, 0x00, 0x80, 0x40,                         // max_frequency = 4.0
];

fn psr_sample() -> g::PowerSwitchRequest {
    g::PowerSwitchRequest {
        request_id: g::PowerPortConfigRequest {
            requesting_device_id: s32("a"),
            target_device_id: s32("bb"),
            config: g::ConfigId::Defaults,
            port_number: 5,
        },
        sequence_id: 1,
        continuity: g::DesiredCircuitContinuity::Closed,
        switch_conditions: Some(g::PowerSwitchCloseConditions {
            min_voltage: 1.0,
            max_voltage: 2.0,
            min_frequency: 3.0,
            max_frequency: 4.0,
        }),
    }
}

#[test]
fn psr_encode_matches_golden() {
    let bytes = encode::<g::PowerSwitchRequest, PsrChain>(&psr_sample());
    assert_eq!(bytes.as_slice(), &PSR_GOLDEN[..]);
}

#[test]
fn psr_decode_golden_matches_sample() {
    let decoded = decode_power_switch_request(&PSR_GOLDEN).unwrap();
    assert_eq!(decoded, psr_sample());
}

#[test]
fn psr_round_trip() {
    let sample = psr_sample();
    let bytes = encode::<g::PowerSwitchRequest, PsrChain>(&sample);
    assert_eq!(decode_power_switch_request(&bytes).unwrap(), sample);
}

// ------------------------------------------------------------------------------------------------
// counting == emit for the composed request objects
// ------------------------------------------------------------------------------------------------

#[test]
fn counting_matches_emit_for_requests() {
    let ess = ess_sample();
    let mut c = CdrCountingSink::new();
    emit_object_cdr::<g::EnergyStartStopRequest, EssChain, _>(&ess, &mut c);
    // encode() length includes finalize padding (3); the pre-finalize payload is 49.
    assert_eq!(c.len(), 49);

    let psr = psr_sample();
    let mut c = CdrCountingSink::new();
    emit_object_cdr::<g::PowerSwitchRequest, PsrChain, _>(&psr, &mut c);
    assert_eq!(c.len(), 72); // already 4-aligned, no finalize padding
}

// ------------------------------------------------------------------------------------------------
// Decode error paths
// ------------------------------------------------------------------------------------------------

#[test]
fn decode_rejects_bad_encapsulation() {
    let mut bad = ESS_GOLDEN;
    bad[1] = 0x02; // PL_CDR_BE-ish — not an accepted LE XCDR2 identifier
    assert_eq!(
        decode_energy_start_stop_request(&bad),
        Err(CdrError::BadEncapsulation)
    );
}

#[test]
fn decode_rejects_truncated_before_top_dheader() {
    // Only 2 bytes after the 4-byte encapsulation header: the top-level DHEADER u32 can't be read.
    assert_eq!(
        decode_energy_start_stop_request(&ESS_GOLDEN[..6]),
        Err(CdrError::Truncated)
    );
}

#[test]
fn decode_rejects_dheader_longer_than_buffer() {
    // The top-level DHEADER (41) declares more bytes than the truncated buffer holds → BadLength.
    assert_eq!(
        decode_energy_start_stop_request(&ESS_GOLDEN[..10]),
        Err(CdrError::BadLength)
    );
}

#[test]
fn decode_accepts_cdr2_le_identifier() {
    // A writer that tags the payload CDR2_LE {0x00,0x07} instead of D_CDR2_LE is still accepted.
    let mut alt = ESS_GOLDEN;
    alt[1] = 0x07;
    assert_eq!(
        decode_energy_start_stop_request(&alt).unwrap(),
        ess_sample()
    );
}

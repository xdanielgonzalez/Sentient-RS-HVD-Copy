//! Steps for the `rtps_spec` feature area: XCDR2/RTPS wire-encoding conformance (DDS-XTypes 1.3,
//! DDSI-RTPS 2.3), story #321 WP7 (#325). Every scenario renders a REAL `lib_tms`/`lib_mapping` CDR
//! chain (`lib_mapping::render_cdr::render_cdr_value`) and asserts on the ACTUAL emitted bytes — no
//! byte value here is copied from the implementation without an independent, in-step derivation
//! (self-consistency formulas, byte-search-and-locate, or a differencing technique), matching this
//! suite's existing convention of reading real enum discriminants via `as u8` rather than hand-
//! copying them (see `steps/tms.rs`).
//!
//! Oracle: `crates/lib-tms/src/cdr/mod.rs`'s module doc (every numbered "Resolved XCDR2 verify-item"
//! there cites the exact DDS-XTypes 1.3 (formal/2020-02-04) / DDSI-RTPS 2.3 section this file's
//! scenarios exercise) + the WP5/WP7a CycloneDDS 11.0.1 empirical arbitration
//! (`deploy/dds-probe/dump_reference.py`, `deploy/dds-conformance/`) that SETTLED the two items that
//! were open questions earlier in the round (optional-presence encoding, rule-12 NEXTINT) — every
//! scenario here is therefore written as CONFIRMED, not `@provisional`.
//!
//! Pure in-process: constructs real `lib_tms` types + calls the real CDR chain, no DDS runtime, no
//! network, no Seplos decode.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cucumber::{given, then, when};

use lib_mapping::render_cdr::render_cdr_value;
use lib_tms::cdr::HasCdrChain;
use lib_tms::{
    ActiveMicrogridControllerState, ClockMonotonic, ConfigId, DcLoadSharingState,
    DcMeasurementUpdate, DcPowerPortMeasurement as PowerPortMeasurement, DeviceConfigRequest,
    EnergyStartStopLevel, EnergyStartStopRequest, Heartbeat, Identity,
};

use crate::SpecWorld;

/// Scenario state for the `rtps_spec` feature area: the most recently rendered payload(s) (`payload`
/// is always populated; `payload_b`/probe flags are populated only by the scenarios that need a
/// second render or a multi-render probe).
#[derive(Debug, Default)]
pub struct RtpsState {
    payload: Vec<u8>,
    payload_b: Vec<u8>,
    /// Set by the `sequence_id` alignment probe (`when_seq_id_alignment_probed`): `true` once every
    /// probed length aligned to 4 AND at least one probed length was demonstrably NOT 8-aligned.
    alignment_confirmed_4_not_8: Option<bool>,
}

fn ident(s: &str) -> Identity {
    let mut i = Identity::new();
    i.push_str(s).expect("fits Identity (<=32 chars)");
    i
}

fn render<T: HasCdrChain>(v: &T) -> Vec<u8> {
    let mut buf = [0u8; 4096];
    let n = render_cdr_value(v, &mut buf).expect("render fits the scratch buffer");
    buf[..n].to_vec()
}

fn hex_bytes(spec: &str) -> Vec<u8> {
    spec.split_whitespace()
        .map(|tok| {
            let tok = tok.trim_start_matches("0x").trim_start_matches("0X");
            u8::from_str_radix(tok, 16).unwrap_or_else(|e| panic!("bad hex byte {tok:?}: {e}"))
        })
        .collect()
}

/// Structural XCDR2 envelope self-consistency (see `test-e2e/tests/dds_interop_e2e.rs`'s identically-
/// reasoned `assert_valid_xcdr2_envelope` — duplicated here rather than shared, matching this
/// workspace's established per-file e2e/spec-suite helper-duplication convention). Returns
/// `(dheader_ssize, trailing_pad, end_of_members_abs_offset)`.
fn envelope(payload: &[u8]) -> (u32, usize, usize) {
    assert!(payload.len() >= 8, "payload too short: {payload:02x?}");
    assert_eq!(
        &payload[0..2],
        &lib_tms::cdr::ENCAPSULATION_D_CDR2_LE,
        "must begin with D_CDR2_LE"
    );
    let pad = (payload[3] & 0x03) as usize;
    let ssize = u32::from_le_bytes([payload[4], payload[5], payload[6], payload[7]]);
    assert_eq!(
        8 + ssize as usize + pad,
        payload.len(),
        "DHEADER/pad framing mismatch"
    );
    (ssize, pad, 8 + ssize as usize)
}

// ---- Given: render a real chain value, stash the raw bytes ------------------------------------

#[given(expr = "the rendered XCDR2 payload for Heartbeat device {string} sequence {int}")]
fn given_heartbeat(world: &mut SpecWorld, device_id: String, seq: i64) {
    world.rtps.payload = render(&Heartbeat {
        device_id: ident(&device_id),
        sequence_number: seq as u32,
    });
}

#[given(expr = "the rendered XCDR2 payload for DcLoadSharingState device {string} config {word}")]
fn given_dc_load_sharing(world: &mut SpecWorld, device_id: String, config: String) {
    world.rtps.payload = render(&DcLoadSharingState {
        device_id: ident(&device_id),
        config: config_from_word(&config),
        parameters: heapless::Vec::new(),
    });
}

fn config_from_word(w: &str) -> ConfigId {
    match w {
        "Unknown" => ConfigId::Unknown,
        "Defaults" => ConfigId::Defaults,
        "Active" => ConfigId::Active,
        "OnReboot" => ConfigId::OnReboot,
        "OnCommsLoss" => ConfigId::OnCommsLoss,
        other => panic!("unknown ConfigId variant {other:?} in feature file"),
    }
}

#[given(
    expr = "the rendered XCDR2 payloads for ActiveMicrogridControllerState device {string} master present and absent"
)]
fn given_amc_present_and_absent(world: &mut SpecWorld, device_id: String) {
    world.rtps.payload = render(&ActiveMicrogridControllerState {
        device_id: ident(&device_id),
        master_id: Some(ident("MC-1")),
    });
    world.rtps.payload_b = render(&ActiveMicrogridControllerState {
        device_id: ident(&device_id),
        master_id: None,
    });
}

#[given(
    expr = "the rendered XCDR2 payload for DcMeasurementUpdate device {string} with one internal port and no external ports"
)]
fn given_dc_measurement_one_internal_port(world: &mut SpecWorld, device_id: String) {
    let mut internal: heapless::Vec<PowerPortMeasurement, 4> = heapless::Vec::new();
    internal
        .push(PowerPortMeasurement {
            // A deliberately distinctive i32 value — unlikely to collide with any other 4-byte run
            // in this small payload — used by the Then step to LOCATE this element's bytes by
            // search, rather than a hand-derived absolute offset.
            port_number: 0x0065_4321,
            line: heapless::Vec::new(),
            power_rate_of_change: 1.5,
        })
        .expect("push within cap 4");
    world.rtps.payload = render(&DcMeasurementUpdate {
        device_id: ident(&device_id),
        time_measured: ClockMonotonic {
            seconds: 42,
            nanoseconds: 7,
        },
        external_measurement: heapless::Vec::new(),
        internal_measurement: internal,
    });
}

// ---- When: the alignment probe (mirrors `steps/tms.rs`'s MAXLEN overflow `#[when]` steps — the
// heavy logic lives here, `Then` just checks the stored verdict) ---------------------------------

/// `EnergyStartStopRequest.sequence_id` is a `u64` field preceded by a nested `DeviceConfigRequest`
/// (two variable-length `Identity` strings + a 4-byte enum) — varying `requesting_device_id`'s length
/// walks the byte offset immediately preceding `sequence_id` through every residue mod 4 (and most
/// residues mod 8) WITHOUT hand-deriving any of them: the probe locates `sequence_id`'s 8
/// little-endian bytes by searching for a distinctive marker value, and requires (a) EVERY probed
/// length lands 4-byte aligned (the XCDR2 rule, DDS-XTypes 1.3 §7.4.2: "INT64, UINT64, FLOAT64 ...
/// aligned to 4 rather than 8" — quoted directly in `lib_tms::cdr::types::cdr_sink::CdrSink::u64`'s
/// doc comment) and (b) AT LEAST ONE probed length lands somewhere that is 4-aligned but NOT
/// 8-aligned — which could only happen if the encoder truly caps alignment at 4.
#[when("EnergyStartStopRequest's sequence_id (u64) is probed across a family of device_id lengths")]
fn when_seq_id_alignment_probed(world: &mut SpecWorld) {
    const MARKER: u64 = 0x1122_3344_5566_7788;
    let marker_le = MARKER.to_le_bytes();

    let mut all_4_aligned = true;
    let mut any_not_8_aligned = false;

    for la in 1..=9usize {
        let requesting = "A".repeat(la);
        let req = EnergyStartStopRequest {
            request_id: DeviceConfigRequest {
                requesting_device_id: ident(&requesting),
                target_device_id: ident("B"),
                config: ConfigId::Unknown,
            },
            sequence_id: MARKER,
            from_level: EnergyStartStopLevel::Any,
            to_level: EnergyStartStopLevel::Operational,
            switch_conditions: None,
        };
        let payload = render(&req);
        let pos = payload
            .windows(8)
            .position(|w| w == marker_le)
            .unwrap_or_else(|| panic!("marker not found for La={la}: {payload:02x?}"));
        if pos % 4 != 0 {
            all_4_aligned = false;
        }
        if pos % 8 != 0 {
            any_not_8_aligned = true;
        }
    }

    world.rtps.alignment_confirmed_4_not_8 = Some(all_4_aligned && any_not_8_aligned);
}

// ---- Then: byte/hex assertions on the stashed payload(s) ---------------------------------------

#[then(expr = "bytes {int}-{int} of the rendered payload equal hex {string}")]
fn then_bytes_range_hex(world: &mut SpecWorld, start: i64, end_inclusive: i64, hex: String) {
    let (start, end) = (start as usize, end_inclusive as usize + 1);
    let got = &world.rtps.payload[start..end];
    assert_eq!(
        got,
        hex_bytes(&hex).as_slice(),
        "bytes {start}-{}: got {got:02x?}",
        end_inclusive
    );
}

#[then(expr = "byte {int} of the rendered payload equals hex {string}")]
fn then_byte_hex(world: &mut SpecWorld, idx: i64, hex: String) {
    let want = hex_bytes(&hex);
    assert_eq!(
        want.len(),
        1,
        "single-byte step must supply exactly one hex byte"
    );
    assert_eq!(world.rtps.payload[idx as usize], want[0], "byte {idx}");
}

#[then("the DHEADER of the rendered payload self-consistently frames its members and trailing pad")]
fn then_dheader_self_consistent(world: &mut SpecWorld) {
    envelope(&world.rtps.payload);
}

#[then(
    "the byte where the present/absent payloads first diverge is 0x01 in the present payload and 0x00 in the absent payload"
)]
fn then_optional_presence_diverges(world: &mut SpecWorld) {
    let (present, absent) = (&world.rtps.payload, &world.rtps.payload_b);
    let n = present.len().min(absent.len());
    // Bytes 0-7 (encapsulation + top-level DHEADER) are SKIPPED: the DHEADER's own `ssize` value
    // necessarily differs between the two renders (a present optional's value adds bytes the absent
    // one doesn't have), so it always diverges first and would otherwise mask the real, semantic
    // divergence point — the `<is_present>` boolean itself, somewhere in the members region (byte 8+).
    let idx = (8..n).find(|&i| present[i] != absent[i]).expect(
        "present/absent payloads must diverge somewhere past the header+DHEADER (they \
                 differ in optionality)",
    );
    assert_eq!(
        present[idx], 0x01,
        "present payload's divergence byte (offset {idx}) must be the <is_present> boolean TRUE"
    );
    assert_eq!(
        absent[idx], 0x00,
        "absent payload's divergence byte (offset {idx}) must be the <is_present> boolean FALSE"
    );
}

#[then("EnergyStartStopRequest's sequence_id aligns to 4 bytes but not always to 8")]
fn then_alignment_4_not_8(world: &mut SpecWorld) {
    assert_eq!(
        world.rtps.alignment_confirmed_4_not_8,
        Some(true),
        "expected every probed length 4-aligned AND at least one NOT 8-aligned"
    );
}

#[then(
    expr = "the rendered payload for DcLoadSharingState config {word} is byte-identical to config {word} except for one 4-byte enum run"
)]
fn then_enum_width_four(_world: &mut SpecWorld, config_a: String, config_b: String) {
    let a = render(&DcLoadSharingState {
        device_id: ident("E"),
        config: config_from_word(&config_a),
        parameters: heapless::Vec::new(),
    });
    let b = render(&DcLoadSharingState {
        device_id: ident("E"),
        config: config_from_word(&config_b),
        parameters: heapless::Vec::new(),
    });
    assert_eq!(a.len(), b.len(), "an enum's width must not vary by variant");
    let diffs: Vec<usize> = (0..a.len()).filter(|&i| a[i] != b[i]).collect();
    assert!(
        !diffs.is_empty(),
        "config {config_a} vs {config_b} must actually differ on the wire"
    );
    // The two `ConfigId` ordinals under test are both small (< 256), so their little-endian u32
    // encodings can differ in ONLY the low byte (the high 3 bytes are 0x00 either way) — a byte-run
    // assertion would under-count. Instead: round the first differing byte DOWN to its enclosing
    // 4-byte-aligned field window (every field's start is 4-aligned in XCDR2 — module doc item 4),
    // then prove the enum occupies EXACTLY that 4-byte window: nothing outside it differs, and inside
    // it the little-endian u32 equals the real ordinal for each variant.
    let first = *diffs.first().unwrap();
    let field_start = first - (first % 4);
    let field_end = field_start + 4;
    assert!(
        field_end <= a.len(),
        "the 4-byte-aligned enum window must fit inside the payload"
    );
    assert_eq!(
        &a[..field_start],
        &b[..field_start],
        "bytes before the enum's 4-byte-aligned field must be identical"
    );
    assert_eq!(
        &a[field_end..],
        &b[field_end..],
        "bytes after the enum's 4-byte-aligned field must be identical"
    );
    let value_a = u32::from_le_bytes(a[field_start..field_end].try_into().unwrap());
    let value_b = u32::from_le_bytes(b[field_start..field_end].try_into().unwrap());
    assert_eq!(
        value_a,
        config_from_word(&config_a) as u32,
        "wire value must equal the real enum ordinal"
    );
    assert_eq!(
        value_b,
        config_from_word(&config_b) as u32,
        "wire value must equal the real enum ordinal"
    );
}

#[then(expr = "the string {string} in the rendered payload is length-prefixed and NUL-terminated")]
fn then_string_ulong_nul(world: &mut SpecWorld, needle: String) {
    let payload = &world.rtps.payload;
    let needle_bytes = needle.as_bytes();
    let pos = payload
        .windows(needle_bytes.len())
        .position(|w| w == needle_bytes)
        .unwrap_or_else(|| panic!("{needle:?} not found in payload: {payload:02x?}"));
    assert!(
        pos >= 4,
        "a string body can never start before byte 4 (the encapsulation header)"
    );
    let len_prefix = u32::from_le_bytes([
        payload[pos - 4],
        payload[pos - 3],
        payload[pos - 2],
        payload[pos - 1],
    ]);
    assert_eq!(
        len_prefix as usize,
        needle.len() + 1,
        "the UInt32 length prefix (DDS-XTypes 1.3 §7.4.3.5.3 rule (3)) must equal the string \
         length INCLUDING the NUL terminator"
    );
    assert_eq!(
        payload[pos + needle_bytes.len()],
        0x00,
        "the byte immediately after the string body must be the trailing NUL"
    );
}

#[then("the rendered payload's rule-(12) NEXTINT correctly delimits the internal-port sequence")]
fn then_rule12_nextint(world: &mut SpecWorld) {
    let payload = &world.rtps.payload;
    let (_ssize, _pad, end_of_members) = envelope(payload);

    // Locate the sole element's distinctive port_number marker (see the Given step) to find the
    // element's own DHEADER (immediately before it — PowerPortMeasurement's first field is
    // port_number), the sequence's count (immediately before THAT, must be 1), and rule-(12)'s
    // NEXTINT (immediately before THAT) — none of these positions are hand-derived; they are located
    // relative to the marker, which the Given step chose to be unambiguous in this small payload.
    let marker_le = 0x0065_4321_i32.to_le_bytes();
    let port_pos = payload
        .windows(4)
        .position(|w| w == marker_le)
        .expect("port_number marker not found");
    assert!(
        port_pos >= 12,
        "not enough preceding bytes for DHEADER+count+NEXTINT"
    );

    let element_dheader = u32::from_le_bytes(payload[port_pos - 4..port_pos].try_into().unwrap());
    let count = u32::from_le_bytes(payload[port_pos - 8..port_pos - 4].try_into().unwrap());
    let nextint = u32::from_le_bytes(payload[port_pos - 12..port_pos - 8].try_into().unwrap());

    assert_eq!(count, 1, "the sequence carries exactly one element");
    // The element (its DHEADER + members) is the LAST thing in the payload (internal_measurement is
    // DcMeasurementUpdate's last field, and this Given left external_measurement empty), so the
    // element's own DHEADER value is exactly the remaining byte count to the end of the members
    // region — self-consistent, no hand-derived constant.
    assert_eq!(
        element_dheader as usize,
        end_of_members - port_pos,
        "element DHEADER must equal the element's own remaining member bytes"
    );
    // rule-(12)'s NEXTINT (DDS-XTypes 1.3 §7.4.3.5.3 rule (12)) is the byte length of
    // `{ count : u32 } << { element }*` that follows it — i.e. from right after the NEXTINT field
    // itself to the end of the sequence, which (same last-field argument) is `end_of_members`.
    let nextint_field_end = port_pos - 8; // right after the NEXTINT u32
    assert_eq!(
        nextint as usize,
        end_of_members - nextint_field_end,
        "rule-(12) NEXTINT must equal the byte length of {{count}} << {{elements}} that follows it \
         — CycloneDDS 11.0.1's decoder REJECTS a sequence of non-primitive elements missing this \
         delimiter (WP7a; see lib_tms::cdr module doc item 7)"
    );
}

// ---- KeyHash plumbing (honestly partial — see the feature file's own comment) ------------------

#[given(expr = "the KeyHash plumbing bytes for Heartbeat device {string}")]
fn given_keyhash_plumbing(world: &mut SpecWorld, device_id: String) {
    let hb = Heartbeat {
        device_id: ident(&device_id),
        sequence_number: 0,
    };
    let mut buf = [0u8; 64];
    let mut sink = lib_tms::cdr::CdrWriteSink::new(&mut buf);
    lib_tms::generated_cdr::heartbeat_cdr::emit_key(&hb, &mut sink);
    let n = sink.len();
    world.rtps.payload = buf[..n].to_vec();
}

#[then(expr = "the KeyHash plumbing bytes equal the device_id string encoding of {string}")]
fn then_keyhash_plumbing_equals_device_id_string(world: &mut SpecWorld, device_id: String) {
    // `emit_key` (DDSI-RTPS 2.3 §9.6.3.8 plumbing — see `lib_tms::cdr::emit_key`'s doc comment) is
    // deliberately INCOMPLETE: it serializes the @key-member chain in declaration order with no
    // encapsulation/DHEADER, but through the SAME little-endian sink as every other field — a real
    // KeyHash_t additionally requires a BIG-ENDIAN sink and then either zero-fill-to-16-bytes (max key
    // size <= 128 bits) or a 128-bit MD5 of the result (deferred; only built if the winning DDS
    // adapter needs an explicit KeyHash — `USER_DATA = deviceId` carries instance identity today).
    // This assertion is scoped to what actually exists: Heartbeat's one @key field (`device_id`)
    // round-trips through `emit_key` as the ordinary LE string encoding, nothing more.
    let mut expected_buf = [0u8; 64];
    let mut sink = lib_tms::cdr::CdrWriteSink::new(&mut expected_buf);
    lib_tms::cdr::CdrSink::string(&mut sink, &device_id);
    let n = sink.len();
    let expected = expected_buf[..n].to_vec();
    assert_eq!(
        world.rtps.payload, expected,
        "emit_key's LE plumbing bytes must equal the ordinary string encoding of the sole @key \
         field (device_id) — the BE + 16-byte/MD5 KeyHash_t wrapper is NOT implemented (deferred)"
    );
}

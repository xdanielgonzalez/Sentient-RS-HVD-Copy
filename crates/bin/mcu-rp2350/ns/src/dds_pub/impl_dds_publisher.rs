//! Behavior for [`DdsPublisher`]: derive the per-device GUID/locator/identity, hand-encode the SEDP
//! `DiscoveredWriterData` announcement, and encode the periodic DATA sample into caller-owned
//! buffers. Every offset/id/flag/length is sourced from the dust_dds wire format and DDSI-RTPS 2.4 /
//! DDS-XTypes 1.3 (citations inline, and on `super`'s constants). No allocation or large stack
//! locals: framing goes through [`ByteCursor`] into reused `'static` buffers; the XCDR2 payload
//! comes from `lib_mapping::render_cdr` into caller-owned scratch.

use lib_mapping::render_cdr::render_tms_cdr;
use lib_mapping::schema::{CanonicalReading, ClockMonotonic, Identity};
use lib_mapping::TmsTopic;

use super::types::DdsPublisher;
use super::{
    DURABILITY_VOLATILE, ENTITYID_SEDP_PUB_WRITER, HEARTBEAT_FLAGS_LE, PID_DATA_REPRESENTATION,
    PID_DURABILITY, PID_ENDPOINT_GUID, PID_MULTICAST_LOCATOR, PID_RELIABILITY, PID_TOPIC_NAME,
    PID_TYPE_NAME, PID_UNICAST_LOCATOR, RELIABILITY_BEST_EFFORT, SUBMSG_HEARTBEAT, TOPIC_NAME,
    TYPE_NAME, USER_WRITER_ENTITYID, XCDR2_REPRESENTATION,
};
// RTPS framing shared with the SPDP encoder (same participant, same submessage shapes).
use crate::spdp::types::ByteCursor;
use crate::spdp::{
    DATA_FLAGS_LE, DEFAULT_MC_PORT, DEFAULT_UC_PORT, ENTITYID_UNKNOWN, INFO_TS_FLAGS_LE,
    LOCATOR_KIND_UDP_V6, MULTICAST_ADDR, PID_SENTINEL, PL_CDR_LE, PROTOCOL_VERSION_2_4,
    SUBMSG_DATA, SUBMSG_INFO_TS, VENDOR_ID,
};

/// The byte length of a CDR `string` parameter VALUE: the 4-byte length prefix + the string bytes +
/// the NUL terminator, padded up to the next 4-byte boundary (DDSI-RTPS 2.4 §9.4.2 / XCDR string
/// rule). The enclosing `param_header` length is exactly this (already a 4-multiple).
#[inline]
fn cdr_string_param_len(s: &str) -> u16 {
    let content = s.len() + 1; // bytes + NUL
    let padded = (content + 3) & !3usize;
    (4 + padded) as u16
}

/// Write a CDR `string`: `u32` length (INCLUDING the NUL terminator) LE, the string bytes, the NUL,
/// then zero padding up to the next 4-byte boundary. Total bytes written == [`cdr_string_param_len`].
#[inline]
fn write_cdr_string(c: &mut ByteCursor, s: &str) {
    let content = s.len() + 1;
    let padded = (content + 3) & !3usize;
    c.u32_le(content as u32);
    c.bytes(s.as_bytes());
    // NUL + trailing pad: `padded - s.len()` zero bytes (>= 1, covering the NUL).
    for _ in 0..(padded - s.len()) {
        c.u8(0);
    }
}

/// A representative synthetic battery [`CanonicalReading`] for the FIRST interop proof — fixed,
/// plausible values (4 cells, 2 temps, a modest discharge current). Live telemetry replaces this in
/// a later step; for now it lets the writer emit a well-formed `DcMeasurementUpdate` sample without
/// waiting on the Seplos poll leg.
pub(crate) fn synthetic_reading() -> CanonicalReading {
    let mut cell_voltages_mv = heapless::Vec::new();
    let _ = cell_voltages_mv.extend_from_slice(&[3894u16, 3901, 3888, 3895]);
    let mut temperatures_raw = heapless::Vec::new();
    let _ = temperatures_raw.extend_from_slice(&[3031u16, 3028]);
    CanonicalReading {
        device_address: 0,
        cell_voltages_mv,
        temperatures_raw,
        current_ca: -144,
        total_voltage_cv: 5451,
        remaining_capacity_cah: 13_953,
        typical_capacity_cah: 18_000,
        rated_capacity_cah: 18_000,
        soc_dpct: 775,
        soh_dpct: 1000,
        cycles: 2,
        port_voltage_cv: 5453,
    }
}

impl DdsPublisher {
    /// Derive the stable per-device publication inputs from the 6-byte Ethernet MAC + the crate's
    /// device identity. The GUID prefix + `fe80::` unicast locator are derived via the SAME
    /// `SpdpAnnounce::new` the SPDP leg uses, so the announced writer's participant matches the one
    /// a peer already discovered over SPDP byte-for-byte.
    pub(crate) fn new(mac: [u8; 6], device_id_str: &str) -> Self {
        let a = crate::spdp::types::SpdpAnnounce::new(mac);
        let mut device_id = Identity::new();
        let _ = device_id.push_str(device_id_str);
        Self {
            guid_prefix: a.guid_prefix,
            unicast_addr: a.unicast_addr,
            device_id,
        }
    }

    /// Hand-encode the SEDP **publication announcement** (`DiscoveredWriterData`) for the
    /// `DcMeasurementUpdate` writer into `buf`, returning its byte length. `seq` is the builtin SEDP
    /// publications writer's `writerSN` (monotonic, starts at 1); `hb_count` is the monotonic
    /// `HeartbeatCount` for the trailing HEARTBEAT (starts at 1, ++ each announce).
    ///
    /// Structure (DDSI-RTPS 2.4 §8.5.4 / dust_dds
    /// `discovered_writer_data.rs`): RTPS `Header` (same participant GuidPrefix) ++ `INFO_TS` ++ a
    /// `DATA` submessage on `ENTITYID_SEDP_BUILTIN_PUBLICATIONS_WRITER` (`00 00 03 c2`) whose
    /// `serializedPayload` is a `PL_CDR_LE` ParameterList ++ a `HEARTBEAT` submessage on that same
    /// writer. Sent to the metatraffic multicast group `ff02::1234:7400` (where a peer's builtin SEDP
    /// reader already listens — proven by SPDP).
    ///
    /// **Why the HEARTBEAT (builtin-SEDP reliability):** the builtin SEDP publications endpoints are
    /// RELIABLE (fixed by DDSI-RTPS, independent of the user-data QoS). A reliable SEDP reader will
    /// not deliver our `DiscoveredWriterData` sample to its discovery layer until a HEARTBEAT
    /// establishes the writer's available sequence-number range — DATA-only leaves it `out-of-sync`
    /// forever and our user writer is never discovered. We append a minimal `Final` HEARTBEAT
    /// (`firstSN = 1 .. lastSN = seq`, same `seq` the DATA carried) so an in-sync reader delivers
    /// immediately; `Final` means we need not service the ACKNACK leg.
    ///
    /// **Test-QoS simplification (documented):** the announced *user-data* QoS is BEST_EFFORT /
    /// VOLATILE, NOT the production TMS Annex-D.5 profile (`lib_tms::generated_qos`). Best-effort
    /// sidesteps the reliable-writer HEARTBEAT/ACKNACK state machine on the USER writer for this
    /// interop proof — a matching best-effort test reader receives the sample directly. (The
    /// HEARTBEAT above is on the BUILTIN SEDP writer, whose reliability is not negotiable.)
    pub(crate) fn encode_sedp(&self, seq: i64, hb_count: i32, buf: &mut [u8]) -> usize {
        let mut c = ByteCursor::new(buf);

        // ---- (1) RTPS Header (20 bytes) — SAME participant GuidPrefix as the SPDP announce. ----
        c.bytes(b"RTPS");
        c.bytes(&PROTOCOL_VERSION_2_4);
        c.bytes(&VENDOR_ID);
        c.bytes(&self.guid_prefix);

        // ---- (2) INFO_TS submessage: header (4) + zeroed Time (8). ----
        c.u8(SUBMSG_INFO_TS);
        c.u8(INFO_TS_FLAGS_LE);
        c.u16_le(8);
        c.u32_le(0);
        c.u32_le(0);

        // ---- (3) DATA submessage on the builtin SEDP publications writer. ----
        c.u8(SUBMSG_DATA);
        c.u8(DATA_FLAGS_LE); // 0x05 (EndiannessFlag | DataFlag)
        let len_at = c.pos();
        c.u16_le(0); // submessageLength placeholder (back-patched)
        let body_start = c.pos();
        c.u16_le(0); // extraFlags
        c.u16_le(16); // octetsToInlineQos (const 16)
        c.bytes(&ENTITYID_UNKNOWN); // readerId
        c.bytes(&ENTITYID_SEDP_PUB_WRITER); // writerId = 00 00 03 c2
        c.i32_le((seq >> 32) as i32); // writerSN.high
        c.u32_le(seq as u32); // writerSN.low

        // ---- (4) serializedPayload: PL_CDR_LE header + the DiscoveredWriterData ParameterList. ----
        c.bytes(&PL_CDR_LE); // 00 03 00 00

        // PID_ENDPOINT_GUID (0x5a), 16 = GuidPrefix(12) ++ the user writer EntityId(4).
        c.param_header(PID_ENDPOINT_GUID, 16);
        c.bytes(&self.guid_prefix);
        c.bytes(&USER_WRITER_ENTITYID);

        // PID_TOPIC_NAME (0x05) = "DcMeasurementUpdate".
        c.param_header(PID_TOPIC_NAME, cdr_string_param_len(TOPIC_NAME));
        write_cdr_string(&mut c, TOPIC_NAME);

        // PID_TYPE_NAME (0x07) = the ::-scoped IDL type "tms::dc::MeasurementUpdate".
        c.param_header(PID_TYPE_NAME, cdr_string_param_len(TYPE_NAME));
        write_cdr_string(&mut c, TYPE_NAME);

        // PID_RELIABILITY (0x1a), 12 = kind(4) ++ max_blocking_time Duration{sec,nanosec}(8).
        // kind = BEST_EFFORT (1) — the wire value dust_dds pins at `ReliabilityQosPolicyKind`.
        c.param_header(PID_RELIABILITY, 12);
        c.u32_le(RELIABILITY_BEST_EFFORT);
        c.i32_le(0); // max_blocking_time.sec
        c.u32_le(100_000_000); // max_blocking_time.nanosec (100 ms — irrelevant for best-effort)

        // PID_DURABILITY (0x1d), 4 = kind. VOLATILE (0).
        c.param_header(PID_DURABILITY, 4);
        c.u32_le(DURABILITY_VOLATILE);

        // PID_DATA_REPRESENTATION (0x73), 8 = sequence<int16>{ XCDR2 } — length(4) ++ value(2) ++
        // pad(2). Our payload is D_CDR2_LE (XCDR2); the TMS type is APPENDABLE, whose XTypes default
        // representation is XCDR2, so an idlc-generated reader matches this offer.
        c.param_header(PID_DATA_REPRESENTATION, 8);
        c.u32_le(1); // sequence length
        c.u16_le(XCDR2_REPRESENTATION); // XCDR2 = 2
        c.u16_le(0); // pad to 4

        // PID_UNICAST_LOCATOR (0x2f): this writer's fe80:: : 7411 (a reader may reply/repair here).
        c.param_header(PID_UNICAST_LOCATOR, 24);
        c.locator(
            LOCATOR_KIND_UDP_V6,
            DEFAULT_UC_PORT as u32,
            &self.unicast_addr,
        );

        // PID_MULTICAST_LOCATOR (0x30): ff02::1234 : 7401 — the group this writer's DATA is sent to;
        // a reader joins it to receive our samples.
        c.param_header(PID_MULTICAST_LOCATOR, 24);
        c.locator(LOCATOR_KIND_UDP_V6, DEFAULT_MC_PORT as u32, &MULTICAST_ADDR);

        // PID_SENTINEL (0x01), length 0 — terminates the ParameterList.
        c.param_header(PID_SENTINEL, 0);

        let body_len = c.pos() - body_start;
        c.patch_u16_le(len_at, body_len as u16);

        // ---- (5) HEARTBEAT submessage on the SAME builtin SEDP publications writer ----
        // (DDSI-RTPS 2.4 §8.3.7.5; layout `vendor/dust_dds/src/rtps_messages/submessages/heartbeat.rs`
        // `write_submessage_elements_into_bytes`: readerId, writerId, firstSN, lastSN, count).
        c.u8(SUBMSG_HEARTBEAT); // 0x07
        c.u8(HEARTBEAT_FLAGS_LE); // 0x03 (EndiannessFlag | FinalFlag)
        let hb_len_at = c.pos();
        c.u16_le(0); // octetsToNextHeader placeholder (back-patched to 28)
        let hb_body_start = c.pos();
        c.bytes(&ENTITYID_UNKNOWN); // readerId = 00 00 00 00 (any matched reader accepts)
        c.bytes(&ENTITYID_SEDP_PUB_WRITER); // writerId = 00 00 03 c2 (matches the DATA's writerId)
        c.i32_le(0); // firstSN.high — firstSN = 1
        c.u32_le(1); // firstSN.low
        c.i32_le((seq >> 32) as i32); // lastSN.high — lastSN = seq (same as the DATA's writerSN)
        c.u32_le(seq as u32); // lastSN.low
        c.i32_le(hb_count); // count (HeartbeatCount, Long; ++ each announce)
        let hb_body_len = c.pos() - hb_body_start;
        c.patch_u16_le(hb_len_at, hb_body_len as u16);

        c.pos()
    }

    /// Render `reading` as a `DcMeasurementUpdate` XCDR2 payload (via `lib_mapping::render_cdr`) into
    /// `payload`, then hand-encode a full RTPS `DATA` sample carrying it into `buf`. Returns the
    /// `DATA` datagram byte length, or `0` if the payload render overflowed `payload` (logged by the
    /// caller). `seq` is the USER writer's `writerSN` (monotonic per sample); `now` stamps both the
    /// TMS `timeMeasured` field and the RTPS `INFO_TS`.
    ///
    /// The serializedPayload is the `render_tms_cdr` output copied verbatim — it ALREADY carries its
    /// own XCDR2 encapsulation header (`00 09` D_CDR2_LE) + DHEADER, so it is a ready RTPS
    /// SerializedPayload and is NOT wrapped again.
    pub(crate) fn encode_data(
        &self,
        seq: i64,
        now: ClockMonotonic,
        reading: &CanonicalReading,
        payload: &mut [u8],
        buf: &mut [u8],
    ) -> usize {
        // Serialize-on-demand: canonical reading -> TMS XCDR2 into the caller's reused scratch.
        let plen = match render_tms_cdr(
            TmsTopic::DcMeasurementUpdate,
            self.device_id.clone(),
            reading,
            now,
            payload,
        ) {
            Ok(n) => n,
            Err(_) => return 0,
        };

        let mut c = ByteCursor::new(buf);

        // ---- (1) RTPS Header — SAME participant GuidPrefix. ----
        c.bytes(b"RTPS");
        c.bytes(&PROTOCOL_VERSION_2_4);
        c.bytes(&VENDOR_ID);
        c.bytes(&self.guid_prefix);

        // ---- (2) INFO_TS submessage: header (4) + Time (8) stamped with `now`. ----
        c.u8(SUBMSG_INFO_TS);
        c.u8(INFO_TS_FLAGS_LE);
        c.u16_le(8);
        c.u32_le(now.seconds); // Time.seconds
        c.u32_le(0); // Time.fraction

        // ---- (3) DATA submessage on the USER writer. ----
        c.u8(SUBMSG_DATA);
        c.u8(DATA_FLAGS_LE); // 0x05 (EndiannessFlag | DataFlag) — no inline QoS
        let len_at = c.pos();
        c.u16_le(0); // submessageLength placeholder
        let body_start = c.pos();
        c.u16_le(0); // extraFlags
        c.u16_le(16); // octetsToInlineQos
        c.bytes(&ENTITYID_UNKNOWN); // readerId
        c.bytes(&USER_WRITER_ENTITYID); // writerId = 00 01 00 02 (user, with-key)
        c.i32_le((seq >> 32) as i32); // writerSN.high
        c.u32_le(seq as u32); // writerSN.low

        // ---- (4) serializedPayload: the render_tms_cdr bytes verbatim (own XCDR2 encapsulation). ----
        c.bytes(&payload[..plen]);

        let body_len = c.pos() - body_start;
        c.patch_u16_le(len_at, body_len as u16);
        c.pos()
    }
}

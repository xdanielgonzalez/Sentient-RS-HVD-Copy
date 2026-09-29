//! Behavior for [`SpdpAnnounce`]: derive the per-device GUID prefix + unicast locator from the MAC
//! ([`new`](SpdpAnnounce::new)), and hand-encode the full RTPS SPDP participant-announcement packet
//! into a caller-owned buffer ([`encode`](SpdpAnnounce::encode)).
//!
//! Every offset, id, flag, and length below is sourced from the dust_dds wire format
//! (`vendor/dust_dds/`) and the DDSI-RTPS 2.4 spec; the citations live inline and in `super`'s
//! constant docs. Nothing here allocates or takes a large stack local — the packet is written
//! straight through a [`ByteCursor`] over the caller's reused `'static` buffer.

use super::types::{ByteCursor, SpdpAnnounce};
use super::{
    BUILTIN_ENDPOINT_SET, DATA_FLAGS_LE, DEFAULT_MC_PORT, DEFAULT_UC_PORT, DOMAIN_ID,
    ENTITYID_PARTICIPANT, ENTITYID_SPDP_WRITER, ENTITYID_UNKNOWN, FLAGS_LE, INFO_TS_FLAGS_LE,
    LEASE_SECS, LOCATOR_KIND_UDP_V6, META_MC_PORT, META_UC_PORT, MULTICAST_ADDR,
    PID_BUILTIN_ENDPOINT_SET, PID_DEFAULT_MULTICAST_LOCATOR, PID_DEFAULT_UNICAST_LOCATOR,
    PID_DOMAIN_ID, PID_METATRAFFIC_MULTICAST_LOCATOR, PID_METATRAFFIC_UNICAST_LOCATOR,
    PID_PARTICIPANT_GUID, PID_PARTICIPANT_LEASE_DURATION, PID_PROTOCOL_VERSION, PID_SENTINEL,
    PID_VENDORID, PL_CDR_LE, PROTOCOL_VERSION_2_4, SUBMSG_DATA, SUBMSG_INFO_TS, VENDOR_ID,
};

impl SpdpAnnounce {
    /// Derive the stable per-device announcement inputs from the 6-byte Ethernet MAC.
    ///
    /// - **GuidPrefix** = `vendorId(2) ++ MAC(6) ++ [0,0,0,1]`. The 2-byte vendor prefix follows the
    ///   RTPS convention that a GuidPrefix opens with the implementer's vendor id (DDSI-RTPS 2.4
    ///   §9.3.1.5); the MAC makes it globally unique; the fixed `[0,0,0,1]` tail is a participant
    ///   instance discriminator (this device hosts exactly one participant), keeping the prefix
    ///   constant across reboots.
    /// - **unicast_addr** = the `fe80::` link-local for this MAC, byte-for-byte what
    ///   `lib_net::embassy::link_local_v6` (and therefore the real interface under SLAAC) produces:
    ///   `fe80::/64` prefix ++ modified-EUI-64 interface id (RFC 4291 §2.5.1; U/L bit of the first
    ///   MAC octet flipped, `0xff 0xfe` inserted mid-MAC).
    pub(crate) fn new(mac: [u8; 6]) -> Self {
        let guid_prefix = [
            VENDOR_ID[0],
            VENDOR_ID[1],
            mac[0],
            mac[1],
            mac[2],
            mac[3],
            mac[4],
            mac[5],
            0x00,
            0x00,
            0x00,
            0x01,
        ];

        // Modified EUI-64 interface identifier (RFC 4291 §2.5.1) — matches link_local_v6.
        let unicast_addr = [
            0xfe,
            0x80,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            mac[0] ^ 0x02, // flip the Universal/Local bit
            mac[1],
            mac[2],
            0xff,
            0xfe,
            mac[3],
            mac[4],
            mac[5],
        ];

        Self {
            guid_prefix,
            unicast_addr,
        }
    }

    /// Hand-encode the complete RTPS SPDP announcement datagram into `buf`, returning its byte
    /// length. `seq` is the DATA submessage's `writerSN` (monotonic per announce, starts at 1).
    ///
    /// Structure (DDSI-RTPS 2.4 §8.3.3 / §8.3.7; byte layout cross-checked against dust_dds):
    /// 1. RTPS `Header` (20 B): `b"RTPS"` ++ ProtocolVersion 2.4 ++ VendorId ++ GuidPrefix
    ///    (`vendor/dust_dds/src/rtps_messages/overall_structure.rs:501-538`).
    /// 2. `INFO_TS` submessage (§8.3.7.9): id `0x09`, LE flag only, an 8-byte zeroed `Time`. The
    ///    timestamp is intentionally fixed/zero — SPDP liveness is driven by the lease duration, not
    ///    the announce timestamp, so a real clock is out of scope for this discovery increment.
    /// 3. `DATA` submessage (§8.3.7.2): id `0x15`, flags `0x05` (Endianness+Data), `extraFlags=0`,
    ///    `octetsToInlineQos=16`, `readerId=ENTITYID_UNKNOWN`,
    ///    `writerId=ENTITYID_SPDP_BUILTIN_PARTICIPANT_WRITER`, `writerSN`, then the serialized
    ///    payload (`vendor/dust_dds/src/rtps_messages/submessages/data.rs:147-176`).
    /// 4. serializedPayload = `PL_CDR_LE` header (`00 03 00 00`) ++ a `ParameterList` of
    ///    `(pid,len,value)` triples ending in `PID_SENTINEL` — parameter set + layout mirror
    ///    `vendor/dust_dds/.../spdp_discovered_participant_data.rs:499-565`.
    pub(crate) fn encode(&self, seq: i64, buf: &mut [u8]) -> usize {
        let mut c = ByteCursor::new(buf);

        // ---- (1) RTPS Header (20 bytes) ----
        c.bytes(b"RTPS");
        c.bytes(&PROTOCOL_VERSION_2_4); // [0x02, 0x04]
        c.bytes(&VENDOR_ID);
        c.bytes(&self.guid_prefix);

        // ---- (2) INFO_TS submessage: header (4) + Time (8) ----
        c.u8(SUBMSG_INFO_TS); // 0x09
        c.u8(INFO_TS_FLAGS_LE); // 0x01 (LE, InvalidateFlag clear → timestamp present)
        c.u16_le(8); // submessageLength = sizeof(Time)
        c.u32_le(0); // Time.seconds  (fixed/zero — see doc above)
        c.u32_le(0); // Time.fraction

        // ---- (3) DATA submessage ----
        c.u8(SUBMSG_DATA); // 0x15
        c.u8(DATA_FLAGS_LE); // 0x05 (EndiannessFlag | DataFlag)
                             // submessageLength is back-patched once the body length is known.
        let len_at = c.pos();
        c.u16_le(0); // placeholder
        let body_start = c.pos();
        c.u16_le(0); // extraFlags (always 0)
        c.u16_le(16); // octetsToInlineQos (const 16 — dust_dds data.rs:163)
        c.bytes(&ENTITYID_UNKNOWN); // readerId  = 00 00 00 00
        c.bytes(&ENTITYID_SPDP_WRITER); // writerId = 00 01 00 c2
                                        // writerSN = SequenceNumber { high: Long, low: UnsignedLong } (§9.4.2.5)
        c.i32_le((seq >> 32) as i32);
        c.u32_le(seq as u32);

        // ---- (4) serializedPayload: PL_CDR_LE header + ParameterList ----
        c.bytes(&PL_CDR_LE); // 00 03 00 00

        // PID_PROTOCOL_VERSION (0x15), value {major,minor} + 2 pad = 4 bytes.
        c.param_header(PID_PROTOCOL_VERSION, 4);
        c.bytes(&PROTOCOL_VERSION_2_4);
        c.bytes(&[0x00, 0x00]);

        // PID_VENDORID (0x16), value 2 bytes + 2 pad = 4.
        c.param_header(PID_VENDORID, 4);
        c.bytes(&VENDOR_ID);
        c.bytes(&[0x00, 0x00]);

        // PID_PARTICIPANT_GUID (0x50), value 16 = GuidPrefix(12) ++ EntityId(4). The participant's
        // own entity is ENTITYID_PARTICIPANT (00 00 01 c1).
        c.param_header(PID_PARTICIPANT_GUID, 16);
        c.bytes(&self.guid_prefix);
        c.bytes(&ENTITYID_PARTICIPANT);

        // PID_DOMAIN_ID (0x0f), value u32 = 0 (domain 0). Not strictly required by all readers, but
        // cheap and makes the domain explicit on the wire.
        c.param_header(PID_DOMAIN_ID, 4);
        c.u32_le(DOMAIN_ID);

        // PID_METATRAFFIC_MULTICAST_LOCATOR (0x33): the SPDP discovery group ff02::1234 : 7400.
        c.param_header(PID_METATRAFFIC_MULTICAST_LOCATOR, 24);
        c.locator(LOCATOR_KIND_UDP_V6, META_MC_PORT as u32, &MULTICAST_ADDR);

        // PID_METATRAFFIC_UNICAST_LOCATOR (0x32): this device's fe80:: : 7410 (metatraffic unicast).
        c.param_header(PID_METATRAFFIC_UNICAST_LOCATOR, 24);
        c.locator(LOCATOR_KIND_UDP_V6, META_UC_PORT as u32, &self.unicast_addr);

        // PID_DEFAULT_UNICAST_LOCATOR (0x31): this device's fe80:: : 7411 (user-data unicast).
        c.param_header(PID_DEFAULT_UNICAST_LOCATOR, 24);
        c.locator(
            LOCATOR_KIND_UDP_V6,
            DEFAULT_UC_PORT as u32,
            &self.unicast_addr,
        );

        // PID_DEFAULT_MULTICAST_LOCATOR (0x48): the user-data multicast group ff02::1234 : 7401. A
        // discovering reader joins this to receive our `DcMeasurementUpdate` DATA samples (which
        // `crate::dds_pub` sends to exactly this group+port). See that PID's `super` doc.
        c.param_header(PID_DEFAULT_MULTICAST_LOCATOR, 24);
        c.locator(LOCATOR_KIND_UDP_V6, DEFAULT_MC_PORT as u32, &MULTICAST_ADDR);

        // PID_BUILTIN_ENDPOINT_SET (0x58): advertise the SPDP participant announcer + detector,
        // plus SEDP publications-announcer so peers (e.g. CycloneDDS) expect + match our SEDP
        // `DiscoveredWriterData`. See `BUILTIN_ENDPOINT_SET`'s doc for the full bit rationale.
        c.param_header(PID_BUILTIN_ENDPOINT_SET, 4);
        c.u32_le(BUILTIN_ENDPOINT_SET);

        // PID_PARTICIPANT_LEASE_DURATION (0x02): Duration { seconds: i32, fraction: u32 } = 8 bytes.
        c.param_header(PID_PARTICIPANT_LEASE_DURATION, 8);
        c.i32_le(LEASE_SECS);
        c.u32_le(0);

        // PID_SENTINEL (0x01), length 0 — terminates the ParameterList.
        c.param_header(PID_SENTINEL, 0);

        // Back-patch the DATA submessageLength = bytes written after its 4-byte header.
        let body_len = c.pos() - body_start;
        c.patch_u16_le(len_at, body_len as u16);

        c.pos()
    }
}

// Keep FLAGS_LE referenced even though DATA/INFO_TS use their own composed constants: it documents
// the shared EndiannessFlag base the two derive from (see super's consts). This const-assert also
// pins that both derived flag bytes actually carry the little-endian bit, matching the LE encoding
// this whole module emits.
const _: () = {
    assert!(DATA_FLAGS_LE & FLAGS_LE == FLAGS_LE);
    assert!(INFO_TS_FLAGS_LE & FLAGS_LE == FLAGS_LE);
};

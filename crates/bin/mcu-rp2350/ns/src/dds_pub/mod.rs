//! DDS/RTPS user-data publication leg for the RP2350 device, on top of the SPDP discovery leg in
//! `crate::spdp` (EPIC #328 step 2). Announces a `DcMeasurementUpdate` writer via SEDP
//! (`DiscoveredWriterData`) and periodically publishes a TMS `DcMeasurementUpdate` DATA sample with
//! an XCDR2 payload over UDP/IPv6, so a CycloneDDS `DataReader` (from the idlc-compiled
//! MIL-STD-3071 IDL) matches and decodes it. Reuses the SPDP RTPS framing and
//! [`ByteCursor`](crate::spdp::types::ByteCursor); GUID prefix and `fe80::` locator come from the
//! same `SpdpAnnounce::new`, so this writer belongs to the already-discovered SPDP participant.
//! Topic/type names, QoS, and multicast addressing are documented on the constants below; publish
//! cadence is fixed at 1 Hz per MIL-STD-3071 (see `PUBLISH_INTERVAL`).

use embassy_net::udp::{PacketMetadata, UdpSocket};
use embassy_net::{IpAddress, IpEndpoint, Ipv6Address, Stack};
use embassy_time::{Duration, Instant, Timer};
use lib_mapping::schema::ClockMonotonic;
use static_cell::StaticCell;

pub(crate) mod types;

mod impl_dds_publisher;

use impl_dds_publisher::synthetic_reading;
use types::DdsPublisher;

// ===========================================================================================
// User-data publication constants. The RTPS framing constants (protocol version, vendor id,
// submessage ids/flags, ENTITYID_UNKNOWN, PL_CDR_LE, locator kind, ports, the multicast group) are
// REUSED from `crate::spdp` — this participant is the same one SPDP announces. Only the
// publication-specific EntityIds, PIDs, and QoS enum values are defined here.
// ===========================================================================================

/// The USER writer's `EntityId` (4 bytes). `entityKey = 00 01 00` (a stable in-house key),
/// `entityKind = USER_DEFINED_WRITER_WITH_KEY = 0x02` — WITH_KEY because `DcMeasurementUpdate`'s
/// `deviceId` member is the topic `@key` (IDL `@key tms::Identity deviceId`). See
/// `vendor/dust_dds/src/transport/types.rs:15` (`USER_DEFINED_WRITER_WITH_KEY`).
pub(crate) const USER_WRITER_ENTITYID: [u8; 4] = [0x00, 0x01, 0x00, 0x02];

/// `ENTITYID_SEDP_BUILTIN_PUBLICATIONS_WRITER` — the builtin writer the `DiscoveredWriterData`
/// announce rides (`vendor/dust_dds/src/dcps/domain_participant.rs:136-137`; key `00 00 03`, kind
/// `BUILT_IN_WRITER_WITH_KEY = 0xc2`).
pub(crate) const ENTITYID_SEDP_PUB_WRITER: [u8; 4] = [0x00, 0x00, 0x03, 0xc2];

/// `HEARTBEAT` submessageId (`vendor/dust_dds/src/rtps_messages/types.rs:103` — `HEARTBEAT = 0x07`).
/// Appended after the SEDP `DATA` so the peer's RELIABLE builtin SEDP reader learns our available
/// sequence-number range and delivers the `DiscoveredWriterData` sample to its discovery layer
/// (without a HEARTBEAT the reliable reader stays `out-of-sync` and never matches our user writer).
pub(crate) const SUBMSG_HEARTBEAT: u8 = 0x07;

/// `HEARTBEAT` flags: EndiannessFlag (bit 0) | FinalFlag (bit 1) = `0x03`; LivelinessFlag (bit 2)
/// CLEAR. `Final` means "no ACKNACK response required unless the reader is missing data", so an
/// in-sync reader delivers immediately without us servicing the ACKNACK leg. Bit positions confirmed
/// against `vendor/dust_dds/src/rtps_messages/submessages/heartbeat.rs:30-31` (`flags()[1]` =
/// final_flag, `flags()[2]` = liveliness_flag) and its `serialize_heart_beat` test (final=false,
/// liveliness=true ⇒ `0b0000_0101`, so our final-only variant is `0b0000_0011`).
pub(crate) const HEARTBEAT_FLAGS_LE: u8 = crate::spdp::FLAGS_LE | (1 << 1);

// --- SEDP DiscoveredWriterData PIDs (DDSI-RTPS 2.4 Table 9.13; dust_dds
// `.../data_representation_builtin_endpoints/parameter_id_values.rs`). ---
/// `PID_TOPIC_NAME` (`parameter_id_values.rs:12`).
pub(crate) const PID_TOPIC_NAME: u16 = 0x0005;
/// `PID_TYPE_NAME` (`parameter_id_values.rs:13`).
pub(crate) const PID_TYPE_NAME: u16 = 0x0007;
/// `PID_RELIABILITY` (`parameter_id_values.rs:21`).
pub(crate) const PID_RELIABILITY: u16 = 0x001a;
/// `PID_DURABILITY` (`parameter_id_values.rs:16`).
pub(crate) const PID_DURABILITY: u16 = 0x001d;
/// `PID_UNICAST_LOCATOR` (`parameter_id_values.rs:36`).
pub(crate) const PID_UNICAST_LOCATOR: u16 = 0x002f;
/// `PID_MULTICAST_LOCATOR` (`parameter_id_values.rs:37`).
pub(crate) const PID_MULTICAST_LOCATOR: u16 = 0x0030;
/// `PID_DATA_REPRESENTATION` (`parameter_id_values.rs:60`).
pub(crate) const PID_DATA_REPRESENTATION: u16 = 0x0073;
/// `PID_ENDPOINT_GUID` (`parameter_id_values.rs:53`) — the writer's GUID (the `DiscoveredWriterData`
/// key member).
pub(crate) const PID_ENDPOINT_GUID: u16 = 0x005a;

// --- QoS enum wire values (dust_dds `.../infrastructure/qos_policy.rs`). ---
/// `ReliabilityQosPolicyKind::BestEffort` wire value = 1 (`qos_policy.rs:905`).
pub(crate) const RELIABILITY_BEST_EFFORT: u32 = 1;
/// `DurabilityQosPolicyKind::Volatile` wire value = 0 (`qos_policy.rs:367`, first variant).
pub(crate) const DURABILITY_VOLATILE: u32 = 0;
/// `XCDR2_DATA_REPRESENTATION` = 2 (`qos_policy.rs:1483`) — our payload is D_CDR2_LE (XCDR2).
pub(crate) const XCDR2_REPRESENTATION: u16 = 2;

// --- Topic + type identifiers (idlc / lib_tms ground truth; see the module doc). ---
/// DDS topic name for the `DcMeasurementUpdate` writer.
pub(crate) const TOPIC_NAME: &str = "DcMeasurementUpdate";
/// The `::`-scoped IDL type name idlc uses for that topic's struct
/// (`module tms { module dc { @topic struct MeasurementUpdate } }`).
pub(crate) const TYPE_NAME: &str = "tms::dc::MeasurementUpdate";

/// The metatraffic multicast group `ff02::1234` — SEDP `DiscoveredWriterData` destination (port
/// 7400), same group SPDP uses and a peer's builtin SEDP reader already listens on.
const MULTICAST_GROUP: Ipv6Address = Ipv6Address::new(0xff02, 0, 0, 0, 0, 0, 0, 0x1234);

/// Publication cadence: emit one SEDP announce + one DATA sample every 1 s. Well under the SPDP
/// lease so a matched reader never times the participant/writer out.
///
/// **MUST stay 1 s**: `DcMeasurementUpdate` is spec QoS Profile Medium / Data Trigger "Periodic
/// 1 s" (`docs/compliance/tms/mil-std-3071_data_model.idl:358`, `TOPIC_DC_MEASUREMENT_UPDATE`).
/// This constant is the wire cadence for the DDS demo leg's own synthetic sample (separate from,
/// but the same defect class as, `run_config()`'s `interval_ms` in `main.rs` — see that constant's
/// doc comment). A previous value of 2 s under-published relative to the 1 Hz requirement; see
/// `PUBLISH_INTERVAL_MS` below for the compile-time gate.
const PUBLISH_INTERVAL: Duration = Duration::from_secs(1);

/// Millisecond mirror of [`PUBLISH_INTERVAL`] purely so the value can be compared in a `const`
/// assertion (`Duration::from_secs` / `Duration::as_secs` are not usable in `const` contexts the
/// way this crate's MSRV needs, so the raw integer is asserted instead, matching the
/// `interval_ms`/`RunConfig` convention in `main.rs`).
const PUBLISH_INTERVAL_MS: u64 = PUBLISH_INTERVAL.as_millis();

const _: () = assert!(
    PUBLISH_INTERVAL_MS == 1000,
    "MIL-STD-3071 requires 1 Hz DcMeasurementUpdate publication (QoS Profile Medium / Periodic 1 s, \
     docs/compliance/tms/mil-std-3071_data_model.idl:358) — do not change PUBLISH_INTERVAL away from 1 s"
);

// ===========================================================================================
// Static socket + buffer storage. As with SPDP, embassy-net's `UdpSocket` borrows its buffers for
// its whole life, so they must be `'static` (repo zero-copy/stack-discipline rule — no per-call
// stack buffers on this path). Separate cells from SPDP's so the two tasks own disjoint storage.
// ===========================================================================================

const DDS_META_CNT: usize = 4;
/// Backing buffer for an encoded SEDP announce or DATA datagram. A DcMeasurementUpdate DATA sample
/// (~a few hundred bytes) and the SEDP announce both fit comfortably; 1024 leaves ample headroom.
const DDS_PKT_LEN: usize = 1024;
/// Scratch for the rendered XCDR2 `DcMeasurementUpdate` SerializedPayload (before it is copied into
/// the DATA submessage). 512 is generous for the synthetic single-port reading.
const DDS_PAYLOAD_LEN: usize = 512;

static DDS_RX_META: StaticCell<[PacketMetadata; DDS_META_CNT]> = StaticCell::new();
static DDS_TX_META: StaticCell<[PacketMetadata; DDS_META_CNT]> = StaticCell::new();
static DDS_RX_BUF: StaticCell<[u8; 256]> = StaticCell::new();
static DDS_TX_BUF: StaticCell<[u8; DDS_PKT_LEN]> = StaticCell::new();
static DDS_PKT_BUF: StaticCell<[u8; DDS_PKT_LEN]> = StaticCell::new();
static DDS_PAYLOAD_BUF: StaticCell<[u8; DDS_PAYLOAD_LEN]> = StaticCell::new();

/// The DDS user-data publication task: announce the `DcMeasurementUpdate` writer via SEDP and
/// publish a TMS `DcMeasurementUpdate` DATA sample, both over UDP/IPv6, every [`PUBLISH_INTERVAL`].
/// `mac` is the device's Ethernet MAC (drives the deterministic GUID prefix + `fe80::` unicast
/// locator — the SAME participant as `crate::spdp`).
///
/// Never returns. Cooperative and non-blocking (the only awaits are timers and multicast
/// `send_to`s), so it cannot starve the broker/telemetry/SPDP tasks on the shared executor.
/// Send-only: no multicast join is needed for egress (the SPDP task already joined `ff02::1234` on
/// the shared interface).
#[embassy_executor::task]
pub(crate) async fn dds_pub_task(stack: Stack<'static>, mac: [u8; 6]) {
    // ---- Wait for the interface to be usable (link-up + SLAAC link-local), then give the SPDP
    // task a head start so a peer discovers the participant before the writer announces.
    while !stack.is_link_up() {
        Timer::after(Duration::from_millis(500)).await;
    }
    Timer::after(Duration::from_secs(2)).await;

    // ---- Open the UDP socket bound to the default user-data unicast port (7411). A single socket
    // sends to both the metatraffic (SEDP) and default (DATA) multicast destinations.
    let rx_meta = DDS_RX_META.init([PacketMetadata::EMPTY; DDS_META_CNT]);
    let tx_meta = DDS_TX_META.init([PacketMetadata::EMPTY; DDS_META_CNT]);
    let rx_buf = DDS_RX_BUF.init([0u8; 256]);
    let tx_buf = DDS_TX_BUF.init([0u8; DDS_PKT_LEN]);
    let pkt = DDS_PKT_BUF.init([0u8; DDS_PKT_LEN]);
    let payload = DDS_PAYLOAD_BUF.init([0u8; DDS_PAYLOAD_LEN]);

    let mut socket = UdpSocket::new(stack, rx_meta, rx_buf, tx_meta, tx_buf);
    if socket.bind(crate::spdp::DEFAULT_UC_PORT).is_err() {
        #[cfg(feature = "defmt")]
        defmt::error!("dds: bind :7411 failed — user-data publication disabled");
        return;
    }

    let publisher = DdsPublisher::new(mac, crate::DEVICE_ID);
    let reading = synthetic_reading();
    let sedp_dest = IpEndpoint::new(IpAddress::Ipv6(MULTICAST_GROUP), crate::spdp::META_MC_PORT);
    let data_dest = IpEndpoint::new(
        IpAddress::Ipv6(MULTICAST_GROUP),
        crate::spdp::DEFAULT_MC_PORT,
    );

    // Per-writer monotonic sequence numbers (the SEDP builtin writer and the user writer each track
    // their own writerSN).
    let mut sedp_seq: i64 = 1;
    let mut data_seq: i64 = 1;
    // The builtin SEDP publications writer's monotonic HeartbeatCount (starts at 1, ++ each announce)
    // — carried by the HEARTBEAT appended to the SEDP DATA so a reliable peer SEDP reader goes
    // in-sync and delivers our DiscoveredWriterData (see `encode_sedp`).
    let mut hb_count: i32 = 1;
    #[cfg(feature = "defmt")]
    let mut first = true;

    loop {
        // ---- SEDP: announce the DcMeasurementUpdate writer (DiscoveredWriterData + HEARTBEAT). ----
        let sedp_len = publisher.encode_sedp(sedp_seq, hb_count, pkt);
        #[cfg(feature = "defmt")]
        if first {
            // Full DATA+HEARTBEAT packet hexdump.
            defmt::info!("dds: sedp[{}] = {=[u8]:02x}", sedp_len, &pkt[..sedp_len]);
        }
        match socket.send_to(&pkt[..sedp_len], sedp_dest).await {
            Ok(()) => {
                #[cfg(feature = "defmt")]
                defmt::info!(
                    "dds: sedp announced {=usize}B seq {=i64} hb {=i32}",
                    sedp_len,
                    sedp_seq,
                    hb_count
                );
            }
            Err(_e) => {
                #[cfg(feature = "defmt")]
                defmt::warn!("dds: sedp send_to failed");
            }
        }
        sedp_seq = sedp_seq.wrapping_add(1);
        hb_count = hb_count.wrapping_add(1);

        // ---- DATA: publish one DcMeasurementUpdate sample (synthetic reading, XCDR2). ----
        let now = ClockMonotonic {
            seconds: Instant::now().as_secs() as u32,
            nanoseconds: 0,
        };
        let data_len = publisher.encode_data(data_seq, now, &reading, payload, pkt);
        if data_len == 0 {
            #[cfg(feature = "defmt")]
            defmt::warn!("dds: render_tms_cdr overflowed payload scratch — sample skipped");
        } else {
            #[cfg(feature = "defmt")]
            if first {
                defmt::info!("dds: data[{}] = {=[u8]:02x}", data_len, &pkt[..data_len]);
            }
            match socket.send_to(&pkt[..data_len], data_dest).await {
                Ok(()) => {
                    #[cfg(feature = "defmt")]
                    defmt::info!("dds: data sent {=usize}B seq {=i64}", data_len, data_seq);
                }
                Err(_e) => {
                    #[cfg(feature = "defmt")]
                    defmt::warn!("dds: data send_to failed");
                }
            }
            data_seq = data_seq.wrapping_add(1);
        }

        #[cfg(feature = "defmt")]
        {
            first = false;
        }
        Timer::after(PUBLISH_INTERVAL).await;
    }
}

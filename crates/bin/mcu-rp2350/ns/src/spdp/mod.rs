//! DDS/RTPS SPDP discovery-announcement leg for the RP2350 device (EPIC #328 step 1 of the MCU DDS
//! path). Joins the SPDP metatraffic multicast group and periodically emits a spec-correct RTPS
//! participant announcement over UDP/IPv6 for peer discovery; user-data DATA interop is the
//! separate `crate::dds_pub` leg.
//!
//! [`spdp_task`] spawns from `main` once the W5500/embassy-net stack is up, waits for link-up +
//! SLAAC, then loops forever, non-blocking, alongside the broker/telemetry tasks. DDS domain 0;
//! SPDP multicast group `ff02::1234` (project-specific, not RTPS-default
//! `239.255.0.1`) — see `docs/tms-compliance.md` (#328/#159) and the port-math constants below.
//! Wire format is hand-encoded from `vendor/dust_dds/` + DDSI-RTPS 2.4 (`impl_spdp_announce.rs`).

use embassy_net::udp::{PacketMetadata, UdpSocket};
use embassy_net::{IpAddress, IpEndpoint, Ipv6Address, Stack};
use embassy_time::{Duration, Timer};
use static_cell::StaticCell;

pub(crate) mod types;

mod impl_byte_cursor;
mod impl_spdp_announce;

use types::SpdpAnnounce;

// ===========================================================================================
// RTPS message framing constants (DDSI-RTPS 2.4). Sourced from dust_dds (`vendor/dust_dds/`) with
// file:line cites; every integer goes on the wire little-endian (the submessage EndiannessFlag is
// set — see `FLAGS_LE`).
// ===========================================================================================

/// RTPS ProtocolVersion 2.4 as the two on-wire bytes `[major, minor]`
/// (`vendor/dust_dds/src/rtps/types.rs:16`, `transport/types.rs:274-289`). Reused verbatim as the
/// `PID_PROTOCOL_VERSION` value.
pub(crate) const PROTOCOL_VERSION_2_4: [u8; 2] = [0x02, 0x04];

/// This device's RTPS VendorId (2 bytes). **Custom / experimental, deliberately not an OMG-assigned
/// vendor id:** `0xFF` as the high byte keeps it clear of the low-numbered real allocations (RTI
/// `01 01`, eProsima `01 0f`, OpenDDS `01 03`, dust_dds `01 14` at
/// `vendor/dust_dds/src/rtps/types.rs:20`), marking this stack as an in-house implementation while
/// avoiding any collision. Documented per the story's "clearly-marked custom one" allowance.
pub(crate) const VENDOR_ID: [u8; 2] = [0xFF, 0x01];

/// Submessage EndiannessFlag base (bit 0 set = little-endian body;
/// `vendor/dust_dds/src/rtps_messages/overall_structure.rs:553`). All other submessage flag bytes
/// below are this OR'd with their submessage-specific bits.
pub(crate) const FLAGS_LE: u8 = 0x01;

/// `INFO_TS` submessageId (`vendor/dust_dds/src/rtps_messages/types.rs:106`).
pub(crate) const SUBMSG_INFO_TS: u8 = 0x09;

/// `INFO_TS` flags: LE only, InvalidateFlag (bit 1) CLEAR ⇒ an 8-byte `Time` payload follows
/// (`vendor/dust_dds/src/rtps_messages/submessages/info_timestamp.rs`).
pub(crate) const INFO_TS_FLAGS_LE: u8 = FLAGS_LE;

/// `DATA` submessageId (`vendor/dust_dds/src/rtps_messages/types.rs:101`).
pub(crate) const SUBMSG_DATA: u8 = 0x15;

/// `DATA` flags: EndiannessFlag (bit 0) | DataFlag (bit 2) = `0x05`; InlineQosFlag (bit 1) clear ⇒
/// no inline QoS, a serializedPayload follows
/// (`vendor/dust_dds/src/rtps_messages/submessages/data.rs:35-38,150-160`).
pub(crate) const DATA_FLAGS_LE: u8 = FLAGS_LE | (1 << 2);

/// `ENTITYID_UNKNOWN` — the DATA readerId (this announce targets no specific reader)
/// (`vendor/dust_dds/src/transport/types.rs:170`).
pub(crate) const ENTITYID_UNKNOWN: [u8; 4] = [0x00, 0x00, 0x00, 0x00];

/// `ENTITYID_SPDP_BUILTIN_PARTICIPANT_WRITER` — the DATA writerId
/// (`vendor/dust_dds/src/dcps/domain_participant.rs:124-125`; key `00 01 00`, kind
/// `BUILT_IN_WRITER_WITH_KEY = 0xc2`).
pub(crate) const ENTITYID_SPDP_WRITER: [u8; 4] = [0x00, 0x01, 0x00, 0xc2];

/// `ENTITYID_PARTICIPANT` — the EntityId tail of `PID_PARTICIPANT_GUID`
/// (`vendor/dust_dds/src/transport/types.rs:171`; key `00 00 01`, kind `BUILT_IN_PARTICIPANT = 0xc1`).
pub(crate) const ENTITYID_PARTICIPANT: [u8; 4] = [0x00, 0x00, 0x01, 0xc1];

/// SerializedPayload representation header for **PL_CDR_LE**: representation_identifier `00 03`
/// (big-endian pair) then zeroed representation_options
/// (`vendor/dust_dds/src/xtypes/serializer.rs:36-41`,
/// `.../spdp_discovered_participant_data.rs:499-500`).
pub(crate) const PL_CDR_LE: [u8; 4] = [0x00, 0x03, 0x00, 0x00];

// ===========================================================================================
// ParameterList PIDs (DDSI-RTPS 2.4 Table 9.13;
// `vendor/dust_dds/src/dcps/data_representation_builtin_endpoints/parameter_id_values.rs`).
// ===========================================================================================

pub(crate) const PID_PARTICIPANT_LEASE_DURATION: u16 = 0x0002; // line 44
pub(crate) const PID_DOMAIN_ID: u16 = 0x000f; // line 32
pub(crate) const PID_PROTOCOL_VERSION: u16 = 0x0015; // line 34
pub(crate) const PID_VENDORID: u16 = 0x0016; // line 35
pub(crate) const PID_DEFAULT_UNICAST_LOCATOR: u16 = 0x0031; // line 38
/// `PID_DEFAULT_MULTICAST_LOCATOR` (`parameter_id_values.rs:39`). Added so a discovering reader
/// learns which multicast group + port this participant's user-data endpoints send to
/// (`ff02::1234:7401`), and joins it to receive the `DcMeasurementUpdate` DATA samples (see
/// `crate::dds_pub`). Without it a reader that only joined the SPDP metatraffic group (7400) would
/// never see our default-multicast (7401) user data.
pub(crate) const PID_DEFAULT_MULTICAST_LOCATOR: u16 = 0x0048; // line 39
pub(crate) const PID_METATRAFFIC_UNICAST_LOCATOR: u16 = 0x0032; // line 40
pub(crate) const PID_METATRAFFIC_MULTICAST_LOCATOR: u16 = 0x0033; // line 41
pub(crate) const PID_PARTICIPANT_GUID: u16 = 0x0050; // line 46
pub(crate) const PID_BUILTIN_ENDPOINT_SET: u16 = 0x0058; // line 48
pub(crate) const PID_SENTINEL: u16 = 0x0001; // line 10

/// `LOCATOR_KIND_UDPv6` (`vendor/dust_dds/src/transport/types.rs:237`). Locator = `{ kind: i32,
/// port: u32, address: [u8;16] }`, all LE except the network-order address.
pub(crate) const LOCATOR_KIND_UDP_V6: i32 = 2;

/// `BuiltinEndpointSet` value advertised: SPDP participant ANNOUNCER (`1<<0`) | DETECTOR (`1<<1`)
/// | SEDP PUBLICATIONS_ANNOUNCER (`1<<2`) = `0x07`
/// (`vendor/dust_dds/.../spdp_discovered_participant_data.rs:44-48`). The publications-announcer
/// bit is required so that CycloneDDS (and other compliant participants) expect an SEDP
/// `DiscoveredWriterData` from us and create a matching `proxy_writer`; without it a peer that
/// only sees `bes 3` (announcer|detector) treats our SEDP publications DATA as unsolicited and
/// silently drops it, so the `DcMeasurementUpdate` writer never matches on the reader side. We do
/// not set the SUBSCRIPTIONS_* or PUBLICATIONS_DETECTOR bits because this participant only
/// publishes SEDP `DiscoveredWriterData` — it neither reads discovery data nor announces any
/// subscriptions.
pub(crate) const BUILTIN_ENDPOINT_SET: u32 = (1 << 0) | (1 << 1) | (1 << 2);

/// DDS domain id (project ground truth: domain 0).
///
/// **MUST stay 0**: MIL-STD-3071 Annex D mandates "All OMG DDS participants SHALL... connect to
/// domain ID 0 by default" (D.2, p.289; see `compliance/oscal/catalog.json:1229-1235`, control
/// citing "MIL-STD-3071 D.1 (p.289); D.2 (p.289); D.3 (p.290)"). The RTPS port math in this module
/// (`SPDP_MULTICAST_PORT` etc., below) is also derived assuming domain 0 (`PB + DG*0 + …`) — a
/// nonzero domain here would desync the port formulas from the constant, not just the spec.
pub(crate) const DOMAIN_ID: u32 = 0;

const _: () = assert!(
    DOMAIN_ID == 0,
    "MIL-STD-3071 Annex D.2 mandates DDS domain ID 0 by default — do not change DOMAIN_ID"
);

/// `PID_PARTICIPANT_LEASE_DURATION` seconds — how long a peer keeps this participant alive without a
/// refresh (DDSI-RTPS default 100 s; `parameter_id_values.rs:72`). The announce interval below is
/// well within it.
pub(crate) const LEASE_SECS: i32 = 100;

// ===========================================================================================
// SPDP transport addressing (project ground truth — domain 0; see the module doc).
// ===========================================================================================

/// SPDP metatraffic multicast port (domain 0): `PB + DG*0 + d0 = 7400`.
pub(crate) const META_MC_PORT: u16 = 7400;
/// Metatraffic unicast port (domain 0): `PB + DG*0 + d1 + PG*participantId = 7410` (participant 0).
pub(crate) const META_UC_PORT: u16 = 7410;
/// Default (user-data) unicast port (domain 0): `PB + DG*0 + d3 + PG*participantId = 7411`.
pub(crate) const DEFAULT_UC_PORT: u16 = 7411;
/// Default (user-data) multicast port (domain 0): `PB + DG*0 + d2 = 7401`. The port our
/// `DcMeasurementUpdate` DATA samples are sent to on the `ff02::1234` group — advertised via
/// `PID_DEFAULT_MULTICAST_LOCATOR` (SPDP) and `PID_MULTICAST_LOCATOR` (the SEDP writer) so a reader
/// joins it. Consumed by both this SPDP encoder and `crate::dds_pub`.
pub(crate) const DEFAULT_MC_PORT: u16 = 7401;

/// The SPDP discovery multicast group `ff02::1234` as the 16 network-order bytes an RTPS `Locator`
/// carries — the `PID_METATRAFFIC_MULTICAST_LOCATOR` address.
pub(crate) const MULTICAST_ADDR: [u8; 16] =
    [0xff, 0x02, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x12, 0x34];

/// The SPDP discovery multicast group as an [`Ipv6Address`] — used for `join_multicast_group` and as
/// the `send_to` destination.
const MULTICAST_GROUP: Ipv6Address = Ipv6Address::new(0xff02, 0, 0, 0, 0, 0, 0, 0x1234);

/// Announce cadence. Comfortably under `LEASE_SECS` so a peer never times the participant out.
const ANNOUNCE_INTERVAL: Duration = Duration::from_secs(3);

// ===========================================================================================
// Static socket + packet storage. embassy-net's `UdpSocket` borrows its rx/tx metadata + payload
// buffers for its whole life, so they must be `'static` — handed out once from `StaticCell`s (repo
// zero-copy/stack-discipline rule; no per-call stack buffers on this path).
// ===========================================================================================

/// UDP packet-metadata ring depth (how many in-flight datagrams the socket tracks). 4 is ample for a
/// single periodic multicast sender that also drains any stray inbound SPDP traffic.
const META_CNT: usize = 4;
/// Backing buffer for the encoded announce packet (~212 B). 512 leaves generous headroom.
const SPDP_PKT_LEN: usize = 512;

static SPDP_RX_META: StaticCell<[PacketMetadata; META_CNT]> = StaticCell::new();
static SPDP_TX_META: StaticCell<[PacketMetadata; META_CNT]> = StaticCell::new();
static SPDP_RX_BUF: StaticCell<[u8; 256]> = StaticCell::new();
static SPDP_TX_BUF: StaticCell<[u8; SPDP_PKT_LEN]> = StaticCell::new();
static SPDP_PKT_BUF: StaticCell<[u8; SPDP_PKT_LEN]> = StaticCell::new();

/// The SPDP discovery task: join `ff02::1234`, then every [`ANNOUNCE_INTERVAL`] emit one RTPS SPDP
/// participant announcement over UDP/IPv6 to `[ff02::1234]:7400`. `mac` is the device's Ethernet MAC
/// (drives the deterministic GuidPrefix + `fe80::` unicast locator; see [`SpdpAnnounce::new`]).
///
/// Never returns. Cooperative and non-blocking: the only awaits are timers and a multicast `send_to`,
/// so this can never starve the broker/telemetry tasks on the shared executor.
#[embassy_executor::task]
pub(crate) async fn spdp_task(stack: Stack<'static>, mac: [u8; 6]) {
    // ---- Wait for the interface to be usable. Link-up first; SLAAC then assigns the fe80::
    // link-local (the address we announce + join from) essentially immediately after.
    while !stack.is_link_up() {
        Timer::after(Duration::from_millis(500)).await;
    }
    Timer::after(Duration::from_secs(1)).await;

    // ---- Join the SPDP multicast group. Retry: the join needs the interface to hold its link-local
    // address, which may land a beat after link-up.
    loop {
        match stack.join_multicast_group(MULTICAST_GROUP) {
            Ok(()) => break,
            Err(_e) => {
                #[cfg(feature = "defmt")]
                defmt::warn!("spdp: join ff02::1234 pending, retrying");
                Timer::after(Duration::from_secs(1)).await;
            }
        }
    }
    #[cfg(feature = "defmt")]
    defmt::info!("spdp: joined ff02::1234");

    // ---- Open the UDP socket bound to the metatraffic multicast port (7400) and prepare the reused
    // packet buffer + the fixed multicast destination.
    let rx_meta = SPDP_RX_META.init([PacketMetadata::EMPTY; META_CNT]);
    let tx_meta = SPDP_TX_META.init([PacketMetadata::EMPTY; META_CNT]);
    let rx_buf = SPDP_RX_BUF.init([0u8; 256]);
    let tx_buf = SPDP_TX_BUF.init([0u8; SPDP_PKT_LEN]);
    let pkt = SPDP_PKT_BUF.init([0u8; SPDP_PKT_LEN]);

    let mut socket = UdpSocket::new(stack, rx_meta, rx_buf, tx_meta, tx_buf);
    if socket.bind(META_MC_PORT).is_err() {
        #[cfg(feature = "defmt")]
        defmt::error!("spdp: bind :7400 failed — SPDP disabled");
        return;
    }

    let announce = SpdpAnnounce::new(mac);
    let dest = IpEndpoint::new(IpAddress::Ipv6(MULTICAST_GROUP), META_MC_PORT);

    // ---- Announce loop: encode once per iteration into the reused buffer, send to the group.
    let mut seq: i64 = 1;
    #[cfg(feature = "defmt")]
    let mut first = true;
    loop {
        let len = announce.encode(seq, pkt);

        // Verification aid: dump the full packet bytes over RTT on the FIRST announce so the wire
        // format can be validated offline against the RTPS spec.
        #[cfg(feature = "defmt")]
        if first {
            defmt::info!("spdp: packet[{}] = {=[u8]:02x}", len, &pkt[..len]);
            first = false;
        }

        match socket.send_to(&pkt[..len], dest).await {
            Ok(()) => {
                #[cfg(feature = "defmt")]
                defmt::info!("spdp: announce sent {=usize}B", len);
            }
            Err(_e) => {
                #[cfg(feature = "defmt")]
                defmt::warn!("spdp: send_to failed");
            }
        }

        seq = seq.wrapping_add(1);
        Timer::after(ANNOUNCE_INTERVAL).await;
    }
}

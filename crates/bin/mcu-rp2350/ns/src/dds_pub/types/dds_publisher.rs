//! [`DdsPublisher`] — per-device, precomputed inputs to the RTPS user-data publication leg: the
//! SEDP `DiscoveredWriterData` announcement for the `DcMeasurementUpdate` writer, plus the periodic
//! DATA sample carrying a TMS XCDR2 payload.
//!
//! Holds the GUID prefix and `fe80::` locator (same participant as SPDP, derived via
//! [`DdsPublisher::new`]) and the `Identity` string the payload is stamped with; everything else is
//! a fixed constant read from `super`. The encoder in `impl_dds_publisher.rs` never allocates.

use lib_mapping::schema::Identity;

/// Precomputed, per-device inputs to the DDS user-data publication. Built once (`new`) and reused
/// for every periodic SEDP announce + DATA sample.
pub(crate) struct DdsPublisher {
    /// The participant's 12-byte RTPS `GuidPrefix` — identical to the SPDP announce's (derived via
    /// the same `SpdpAnnounce::new`), so the announced writer belongs to the participant a peer
    /// already discovered over SPDP.
    pub(crate) guid_prefix: [u8; 12],
    /// The device's own IPv6 link-local (`fe80::`) address as the 16 network-order bytes an RTPS
    /// `Locator` carries — advertised as the writer's unicast locator in the SEDP announce.
    pub(crate) unicast_addr: [u8; 16],
    /// The TMS device identity stamped into every rendered `DcMeasurementUpdate` payload
    /// (`deviceId`, the topic's `@key`). Fixed from the crate's `DEVICE_ID` until a config source
    /// lands on the MCU.
    pub(crate) device_id: Identity,
}

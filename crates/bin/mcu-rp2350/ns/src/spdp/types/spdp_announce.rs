//! [`SpdpAnnounce`] — per-device, precomputed inputs to the SPDP participant announcement (RTPS
//! `SPDPdiscoveredParticipantData`), plus the packet encoder.
//!
//! Holds the two values that vary per device — the 12-byte RTPS `GuidPrefix` and the `fe80::`
//! unicast locator — derived once from the MAC in [`SpdpAnnounce::new`]. Everything else (domain,
//! ports, multicast group, builtin-endpoint bitset, lease duration) is a fixed constant read by the
//! encoder in `impl_spdp_announce.rs`, which never allocates.

/// Precomputed, per-device inputs to the SPDP announcement. Built once (`new`) and reused for every
/// periodic announce; [`encode`](Self::encode) is the only method and is pure over `(self, seq,
/// buf)`.
pub(crate) struct SpdpAnnounce {
    /// The participant's 12-byte RTPS `GuidPrefix` (DDSI-RTPS 2.4 §8.2.4.1). Deterministic per
    /// device: `[vendorId(2)] ++ [MAC(6)] ++ [fixed tail(4)]` — see `new`. Stable across reboots so
    /// a peer re-discovering this device keys it to the same participant GUID.
    pub(crate) guid_prefix: [u8; 12],
    /// The device's own IPv6 link-local (`fe80::`) address as the 16 network-order bytes an RTPS
    /// `Locator` carries (DDSI-RTPS 2.4 §8.3.5.5). Advertised as the metatraffic + default UNICAST
    /// locator so peers can reply directly. A pure function of the MAC (matches
    /// `lib_net::embassy::link_local_v6`) — see `new`.
    pub(crate) unicast_addr: [u8; 16],
}

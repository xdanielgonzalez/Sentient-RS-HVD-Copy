//! The `dust_dds` 0.15.0 backend adapter — the ONLY backend today (WP0/WP4 bake-off winner). All
//! `dust_dds` coupling is confined here; the rest of the crate programs against the seam.
//!
//! - [`DustParticipant`] implements [`crate::DdsParticipant`] (topic/writer creation + QoS map).
//! - [`DustWriter`] implements [`crate::DdsWriter`] — the **rung-C** bytes-in path: our XCDR2 payload
//!   is deserialized into a `DynamicData` (via the topic patch type's `DynamicType`), reconstituted with
//!   `TypeSupport::create_sample`, and re-emitted by the typed `DataWriter` (representation = XCDR2),
//!   round-tripping byte-identically for optional-free topics (WP3 oracle).
//! - [`Ipv6TransportFactory`] is the custom UDP/IPv6 `TransportParticipantFactory` (SPDP `ff02::`
//!   multicast + unicast DATA) — dust_dds's default transport is IPv4-only.

mod prelude;
pub mod types;

mod impl_dust_participant;
mod impl_dust_writer;
mod impl_message_writer;
mod impl_transport_factory;
pub mod qos_map;

pub use types::{DustParticipant, DustWriter, Ipv6MessageWriter, Ipv6TransportFactory};

use crate::types::{DdsConfig, DdsParticipant, Result};

/// Build the dust_dds-backed participant (with the custom UDP/IPv6 transport) for `config`.
pub fn build_participant(config: &DdsConfig) -> Result<Box<dyn DdsParticipant>> {
    Ok(Box::new(DustParticipant::build(config)?))
}

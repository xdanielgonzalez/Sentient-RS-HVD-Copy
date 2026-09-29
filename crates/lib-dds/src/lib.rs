//! # lib-dds — the device's DDS/RTPS (MIL-STD-3071) transport, Pi-only
//!
//! **Platform: Raspberry Pi (Yocto Linux / std + tokio) ONLY. This crate is NEVER compiled for the
//! MCU / `no_std` target** — DDS/RTPS over UDP/IPv6 is a service-layer concern that lives beside the
//! MQTT surface on the Pi (`docs/tms-compliance.md` §7.1). The MCU path stays `no_std` and carries
//! no DDS runtime.
//!
//! ## What it is
//!
//! An **owned seam** ([`DdsParticipant`] / [`DdsWriter`]) over a **single** RTPS backend
//! (`dust_dds` 0.15.0 — the WP0/WP4 bake-off winner). The crate boundary IS the swappability seam: a
//! future backend is a sibling `backend-*` feature + adapter module under the same traits; callers
//! never change. Today `backend-dustdds` is the only (default) backend.
//!
//! ## The pre-serialized-bytes contract (bytes-in seam)
//!
//! [`DdsWriter::write`] takes a **pre-serialized XCDR2 SerializedPayload** — the exact bytes
//! `lib_tms`'s CDR chains emit (`lib_mapping::render_cdr`), encapsulation header + DHEADER + members.
//! The seam is transport-neutral: the poll loop renders once, the adapter puts those bytes on the
//! wire. dust_dds has **no public pre-serialized-bytes hook** (`DataWriter<T>::write` needs
//! `T: TypeSupport`, serialized by its own XCDR2 serializer over a `DynamicData`; the low-level
//! `rtps::stateful_writer::add_change` bypasses discovery/QoS/instance bookkeeping — WP4 rung A/B
//! findings). So the adapter lands on **rung C**: it *deserializes* the bytes into a `DynamicData`
//! keyed by the topic's `DynamicType` and re-emits via the typed path. For every optional-free topic
//! this **round-trips byte-identically** (the WP3 cross-oracle guarantees dust_dds's serialization of
//! that `DynamicData` equals our chains); `@optional`-bearing topics defer to the WP5 CycloneDDS
//! arbitration. See `crates/lib-dds/src/dustdds/impl_dust_writer.rs`.
//!
//! ## IPv6
//!
//! dust_dds's default transport is IPv4-only, but its `TransportParticipantFactory` is a public
//! pluggable seam and `Locator` already carries `LOCATOR_KIND_UDP_V6`. [`dustdds`] ships a custom
//! UDP/IPv6 `TransportParticipantFactory` (SPDP multicast on `ff02::` + unicast DATA), proven to
//! discover + deliver against a containerized CycloneDDS participant over IPv6 (domain 0).

#![forbid(unsafe_code)]

mod prelude;
pub mod types;

mod impl_dds_config;
mod impl_dds_handle;
mod stack;

#[cfg(feature = "backend-dustdds")]
pub mod dustdds;

pub use stack::{dds_channel, run_dds_stack};
pub use types::*;

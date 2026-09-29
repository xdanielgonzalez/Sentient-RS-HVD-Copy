//! The PURE, generated MIL-STD-3071 (TMS) model — machine-generated concrete structs + enums.
//!
//! # A struct generator, not a runtime registry
//!
//! `tools/tms-idl-codegen` parses `docs/compliance/tms/mil-std-3071_data_model.idl` (nom) and EMITS
//! [`generated`] — one concrete Rust `struct`/`enum` per IDL type, plus the IDL typedefs
//! ([`Identity`], [`String32`], …) as named aliases, each type with `serde` derives and (for
//! `@topic` types) a [`TmsTopic`] impl. A generated `generated::TmsMessage` factory enum dispatches
//! inbound bytes by topic. The output is machine-generated from the IDL, so it cannot drift.
//!
//! # This crate is a LEAF of the dependency graph
//!
//! `lib-tms` depends on `heapless` + `serde` ONLY (+ `serde_json` for the host-only decode factory).
//! It has NO canonical dependency: the canonical → wire flat *encoders* (needing both `JsonSink` and
//! the canonical model) live in `lib-mapping` (which depends on this crate), so the layering stays
//! acyclic. [`fields::JsonSink`] itself DOES live here (`crate::fields`) — it has no canonical
//! dependency of its own, only `Src: ?Sized` generics over whatever the caller's encoder borrows —
//! so it is a shared leaf abstraction rather than something `lib-mapping` alone owns. The
//! vendor-neutral device model is the sibling crate `lib-canonical`; the two never depend on each
//! other — `lib-mapping` is the single bridge.
//!
//! # Memory model — topics are a 1-byte index; render on output
//!
//! Topic identity in memory is [`generated::TmsTopicId`] (a `#[repr(u8)]` enum). Topic *strings* are
//! materialized only at I/O boundaries ([`generated::TmsTopicId::as_str`] out,
//! [`generated::TmsTopicId::from_str`] in). Canonical values are stored as a bounded ring buffer
//! (`CanonicalStore`) on the MCU, rendered directly at transmission time (render-on-output).
//!
//! # no_std split
//!
//! The concrete types + [`TmsTopic`] + [`generated::TmsTopicId`] are all `no_std`/no-alloc
//! (`heapless`). Only the serde `TmsMessage::{decode,encode}` factory needs `alloc`/`serde_json` and
//! is gated behind the host-only `std` feature.
//!
//! # Examples
//!
//! Construct a `@topic` type and round-trip it through the exact on-device encoder
//! (`serde-json-core` — `no_std`, bounded buffers, no alloc):
//!
//! ```rust
//! use lib_tms::{Heartbeat, TmsTopic, TmsTopicId};
//!
//! // Bounded heapless fields — the same construction the MCU performs.
//! let hb = Heartbeat {
//!     device_id: heapless::String::try_from("stor-0042")?,
//!     sequence_number: 7,
//! };
//!
//! // Topic identity is the 1-byte index; the string is rendered only at the I/O boundary.
//! assert_eq!(Heartbeat::TOPIC_ID, TmsTopicId::Heartbeat);
//! assert_eq!(Heartbeat::topic_str(), "Heartbeat");
//!
//! // Serialize to the frozen wire spelling, then parse the bytes back — lossless.
//! let json = serde_json_core::to_string::<_, 128>(&hb)?;
//! assert_eq!(json.as_str(), r#"{"device_id":"stor-0042","sequence_number":7}"#);
//! let (back, _len) = serde_json_core::from_str::<Heartbeat>(json.as_str())?;
//! assert_eq!(back, hb);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Enum variants serialize as the exact IDL tokens, and
//! [`wire_str`](generated::CableSenseStatus::wire_str) mirrors the derived `Serialize` spelling
//! for the flat-wire renderer:
//!
//! ```rust
//! use lib_tms::CableSenseStatus;
//!
//! let json = serde_json_core::to_string::<_, 32>(&CableSenseStatus::Connected)?;
//! assert_eq!(json.as_str(), r#""CS_CONNECTED""#);
//! assert_eq!(CableSenseStatus::Connected.wire_str(), "CS_CONNECTED");
//! # Ok::<(), serde_json_core::ser::Error>(())
//! ```
#![no_std]
#![forbid(unsafe_code)]

#[cfg(feature = "std")]
extern crate alloc;

pub mod cdr;
pub mod fields;
pub mod generated;
pub mod generated_cdr;
pub mod generated_fields;
pub mod generated_qos;
mod types;

/// Re-export the whole generated surface at the crate root: every concrete struct/enum, the IDL
/// typedefs (`Identity`, `String32`, …), `TmsTopicId`, and — under the host-only `std` feature — the
/// `TmsMessage` serde dispatch factory (large + needs `serde_json`/`alloc`; the glob picks it up only
/// when `std` is on). Consumers import `lib_tms::DcMeasurementUpdate`, `lib_tms::Identity`, etc.
pub use generated::*;
pub use types::{DecodeError, TmsTopic};

/// TMS data-model release version this model targets (`tms.TMS_VERSION`).
pub const TMS_DATA_MODEL_VERSION: &str = "2.0.2";

/// IDL MAXLENs (SRAM-narrowed sequence bounds shared with the codec/transform paths).
///
/// The bounds are baked into the generated types' `heapless::Vec` capacities, so a sequence
/// cannot exceed its (narrowed) IDL MAXLEN by construction:
///
/// ```rust
/// use lib_tms::{DcPowerLineMeasurement, DcPowerPortMeasurement, MAX_DC_LINES};
///
/// let line = heapless::Vec::from_slice(&[DcPowerLineMeasurement {
///     voltage: 54.52,
///     amperage: 1.64,
///     power: 89.41,
/// }])?;
/// let port = DcPowerPortMeasurement {
///     port_number: 1,
///     line,
///     power_rate_of_change: 0.0,
/// };
/// assert_eq!(port.line.capacity(), MAX_DC_LINES); // bound enforced by the type itself
/// # Ok::<(), heapless::CapacityError>(())
/// ```
pub const MAX_DC_LINES: usize = 3;
pub const MAX_STORAGE_CELLS: usize = 50;
pub const MAX_DTC: usize = 64;

/// TMS DDS topic-name constants — exact string literals from the MIL-STD-3071 IDL `topic` module
/// (IDL 16-404).
///
/// Each constant parses back to its 1-byte [`TmsTopicId`] index —
/// per the render-on-output doctrine, the string form lives only at the I/O boundary:
///
/// ```rust
/// use lib_tms::{topics, TmsTopicId};
///
/// let id = TmsTopicId::from_str(topics::STORAGE_UPDATE).ok_or("unknown TMS topic")?;
/// assert_eq!(id, TmsTopicId::StorageUpdate);
/// assert_eq!(id.as_str(), topics::STORAGE_UPDATE);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub mod topics {
    pub const DC_MEASUREMENT_UPDATE: &str = "DcMeasurementUpdate";
    pub const POWER_HARDWARE_UPDATE: &str = "PowerHardwareUpdate";
    pub const ACTIVE_DIAGNOSTIC_STATE: &str = "ActiveDiagnosticState";
    pub const STORAGE_UPDATE: &str = "StorageUpdate";
    pub const HEARTBEAT: &str = "Heartbeat";
    pub const DEVICE_INFO: &str = "DeviceInfo";
    pub const REPLY: &str = "Reply";
    pub const AC_MEASUREMENT_UPDATE: &str = "AcMeasurementUpdate";
    pub const CONTROL_HARDWARE_UPDATE: &str = "ControlHardwareUpdate";
    pub const ENERGY_START_STOP_STATE: &str = "EnergyStartStopState";
    pub const POWER_PORT_STATE: &str = "PowerPortState";
    pub const ACTIVE_MICROGRID_CONTROLLER_STATE: &str = "ActiveMicrogridControllerState";
    pub const DC_LOAD_SHARING_STATE: &str = "DcLoadSharingState";
    pub const ENERGY_START_STOP_REQUEST: &str = "EnergyStartStopRequest";
    pub const POWER_SWITCH_REQUEST: &str = "PowerSwitchRequest";
}

//! `lib-mapping` — TMS (MIL-STD-3071) payload mapping.
//!
//! Pure, `no_std`-friendly, zero I/O. Maps the vendor-neutral `lib_parse::CanonicalReading`/
//! `CanonicalFault` into TMS typed structs (the mapper consumes only canonical types, never a
//! vendor's). Structs are authored from the authoritative standard IDL (MIL-STD-3071 Change 2,
//! TMS Data Model v2.0.3 — `chapter-d.txt`), not the Python prototype's approximation.
//!
//! * [`schema`] — TMS typed structs + enums + topic-name constants (the swap-seam for the
//!   standard); assembled here from [`lib_tms`] (the generated TMS model) + [`lib_canonical`]
//!   (the device model) so existing call sites keep resolving without churn.
//! * [`transform`] — Canonical → TMS (sign inversion, mV→V, temp aggregation, fault → DTC).
//! * [`codec`] — no_std JSON encode via `serde-json-core`.
//! * [`render`] — serialize-on-demand entry point (canonical -> TMS -> JSON in one call, into a
//!   caller buffer); the seam a render-on-output caller (e.g. a broker rendering a pending publish)
//!   calls through instead of building + serializing TMS structs itself.
//! * [`wire`] — the flat, no-alloc, integer-formatting `Canonical -> TMS-JSON` serializer
//!   (`docs/wire-serialization.md`); `render` calls through this instead of `codec`'s serde path.
//! * [`sink`] — the `Publisher` publish contract + bounded topic builder.
//!
//! AWS-Greengrass backwards-compat is a flat legacy schema relayed by the mosquitto bridge
//! (`distro/deploy/mosquitto.conf`), not a module here.
//!
//! # Example: canonical reading → TMS JSON
//!
//! The render-on-output entry point ([`render_tms`]) maps a compact `CanonicalReading` straight to
//! TMS topic JSON in one call, into a caller-owned buffer — no intermediate TMS struct, no
//! allocation:
//!
//! ```
//! use lib_mapping::schema::{CanonicalReading, ClockMonotonic, Identity};
//! use lib_mapping::{render_tms, TmsTopic};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // A reading in raw vendor units: 54.51 V pack, discharging 1.44 A, 77.5 % SOC.
//! let reading = CanonicalReading {
//!     total_voltage_cv: 5451, // centi-volts
//!     current_ca: -144,       // centi-amps, vendor charge-positive
//!     soc_dpct: 775,          // deci-percent
//!     cell_voltages_mv: heapless::Vec::from_slice(&[3894, 3893, 3895])?, // millivolts
//! #     temperatures_raw: heapless::Vec::from_slice(&[2801, 2811])?,
//! #     device_address: 0,
//! #     remaining_capacity_cah: 13953,
//! #     typical_capacity_cah: 18000,
//! #     rated_capacity_cah: 18000,
//! #     soh_dpct: 1000,
//! #     cycles: 2,
//! #     port_voltage_cv: 5453,
//!     // …remaining raw fields hidden…
//! };
//!
//! let device_id = Identity::try_from("METEOR-001")?;
//! let now = ClockMonotonic { seconds: 1_000, nanoseconds: 0 };
//! let mut buf = [0u8; 1024];
//! let n = render_tms(TmsTopic::DcMeasurementUpdate, device_id, &reading, now, &mut buf)?;
//!
//! // The standard's §2.7 conventions are applied on the way out: centi-volts → volts, and
//! // sign inversion (vendor discharge −1.44 A → TMS into-the-microgrid +1.44 A).
//! let json: serde_json::Value = serde_json::from_slice(&buf[..n])?;
//! assert_eq!(json["device_id"], "METEOR-001");
//! let line = &json["external_measurement"][0]["line"][0];
//! assert_eq!(line["voltage"], 54.51);
//! assert_eq!(line["amperage"], 1.44);
//! # Ok(()) }
//! ```
#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

pub mod codec;
pub mod render;
/// Serialize-on-demand XCDR2 (DDS-XTypes 1.3) rendering for the DDS transport — the CDR twin of
/// [`render`]. See that module's doc comment.
pub mod render_cdr;
pub mod sink;
pub mod transform;
/// Canonical **inverter** → TMS CONV-role conversions (D10, PROVISIONAL — RS-232 Voltronic leg,
/// decision 0001). Sibling of [`transform`]; kept `no_std`/no-alloc the same way.
pub mod transform_inverter;
pub mod wire;

/// AWS-Greengrass backwards-compat flat legacy schema (Scope D) — feature `legacy`, see that
/// module's doc comment.
#[cfg(feature = "legacy")]
pub mod legacy;

pub use render::{render_fault, render_tms, TmsTopic};
pub use sink::{PublishError, Publisher};

/// TMS typed schema — assembled from the split model crates (`lib-tms`'s generated MIL-STD-3071
/// types + `lib-canonical`'s device model) so existing call sites (`lib_mapping::schema::X`) keep
/// resolving without churn.
pub mod schema {
    pub use lib_canonical::*;
    pub use lib_tms::*;

    // The 2 collision aliases: the generator prefixes the `ac`/`dc`-colliding IDL names, but this
    // facade exposes the bare (DC) names some call sites still use.
    pub use lib_tms::DcPowerLineMeasurement as PowerLineMeasurement;
    pub use lib_tms::DcPowerPortMeasurement as PowerPortMeasurement;
}

pub use schema::topics;

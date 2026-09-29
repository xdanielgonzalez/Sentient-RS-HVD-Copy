//! `lib-parse` — feature-bound BMS + inverter vendor protocol parsers.
//!
//! Each battery vendor sits behind a cargo feature and implements the [`vendor::BmsVendorProtocol`]
//! trait: raw response bytes (supplied by an injected transport adapter) → the vendor-neutral
//! [`canonical::CanonicalReading`]. Each inverter vendor likewise implements the sibling
//! [`vendor::InverterVendorProtocol`] trait → [`canonical::CanonicalInverterReading`] (a distinct
//! device class polled over its own RS-232 serial leg — NOT a generalization of the battery
//! trait). The downstream TMS mapper consumes *only* the canonical models, so adding a vendor is a
//! new feature-gated parser — zero mapper change.
//!
//! Pure, `no_std`-friendly (`default-features = false`), zero I/O, zero platform surface. Fixed-point
//! raw units are preserved (no floats); scaling to engineering units is a downstream concern.
//! Correctness bar is the vendor protocol + real device captures, never the Python prototype.
//!
//! # Example: captured Seplos frame → canonical reading
//!
//! Feed the raw bytes of a captured RS-485 `0x42` telemetry response (Seplos 48150-1101-10E,
//! 14-cell pack, DIP address 0) to the vendor parser and get the vendor-neutral
//! [`CanonicalReading`] back — raw fixed-point device units, no floats:
//!
//! ```rust
//! use lib_parse::seplos::Seplos;
//! use lib_parse::BmsVendorProtocol;
//!
//! // A live-captured telemetry response frame (`~`-framed, ASCII-hex, checksum-verified).
//! let frame: &[u8] = b"~20004600A08E00000E0F360F360F360F360F350F360F360F360F360F370F370F36\
//!     0F360F35060AF10AF90AF90AF30B370B0CFF5C154C36820A465003074650000203E8154CFFF8000000\
//!     79004DDEC8\r";
//!
//! let reading = Seplos::new().parse_reading(frame, 0)?;
//! assert_eq!(reading.device_address, 0);
//! assert_eq!(reading.cell_voltages_mv.len(), 14);
//! assert_eq!(reading.cell_voltages_mv[0], 3894); // 0x0F36 mV
//! assert_eq!(reading.current_ca, -164);          // signed centiamps — discharging
//! assert_eq!(reading.total_voltage_cv, 5452);    // centivolts → 54.52 V
//! assert_eq!(reading.soc_dpct, 775);             // deci-percent → 77.5 %
//! # Ok::<(), lib_parse::ParseError>(())
//! ```
#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

pub mod canonical;
pub mod vendor;

/// Seplos BMS protocol parser (RS-485 ASCII, `~`-framed). Enabled by the `seplos` feature.
#[cfg(feature = "seplos")]
pub mod seplos;

/// Voltronic/MPP-Solar PI30 inverter protocol parser (RS-232 ASCII, CRC-16/XMODEM-framed).
/// Enabled by the `voltronic` feature. PROVISIONAL — derived from the legacy Python reference +
/// protocol docs; no real Voltronic/QPIGS wire captures yet (decision 0001's open risk).
#[cfg(feature = "voltronic")]
pub mod voltronic;

pub use canonical::{
    CanonicalFault, CanonicalInverterFault, CanonicalInverterReading, CanonicalReading,
    InverterMode, InverterRatings,
};
pub use vendor::{
    BmsCommand, BmsVendorProtocol, InverterCommand, InverterVendorProtocol, ParseError,
};

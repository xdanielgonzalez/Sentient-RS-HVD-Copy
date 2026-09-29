//! Module-private prelude for this crate's `impl_*.rs` files only (never imported by `src/types/`,
//! per the repo's type/impl split convention — a type file importing this would form a
//! `types ⇄ prelude` glob cycle). Re-exports this crate's own types plus the externals its impls
//! share.

pub(crate) use super::types::*;

pub(crate) use bin_mcu_platform::McuClock;
pub(crate) use core::sync::atomic::Ordering;
pub(crate) use embassy_time::{Instant, Timer};
pub(crate) use lib_mapping::schema::ClockMonotonic;

// Note: MQTT task imports embedded_io_async_07::Write directly (not re-exported here)

// The veneer `SessionCipher` seam's externals (feature "veneers", default OFF) — used only by
// `impl_veneer_session_cipher.rs`.
#[cfg(feature = "veneers")]
pub(crate) use lib_security::{
    KeyCustody, KeyHandle, Result as SecurityResult, SecurityError, SessionCipher,
};
#[cfg(feature = "veneers")]
pub(crate) use rp2350_abi::{
    custody_code_to_key_custody, sv_err_to_security_error, SV_MAX_PAYLOAD,
};

// WP3 (delight-features) mesh-radio leg's externals — used only by `impl_rp2350_mesh_de.rs` /
// `impl_rp2350_mesh_parser.rs`.
#[cfg(feature = "mesh")]
pub(crate) use embedded_hal::digital::{ErrorType, OutputPin};

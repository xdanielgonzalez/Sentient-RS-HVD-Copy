pub(crate) use super::types::*;

pub use std::fs;
pub use std::io::{self, BufRead, Read, Write};
pub use std::net::{TcpListener, TcpStream};
pub use std::path::{Path, PathBuf};
pub use std::time::{Duration, Instant};

pub use lib_mapping::render::{render_fault, render_tms, TmsTopic};
pub use lib_mapping::schema::{ClockMonotonic, Identity};
pub use lib_parse::seplos::Seplos;
pub use lib_parse::vendor::BmsVendorProtocol;
pub use test_kit::seplos::{
    parse_battery_state_spec, parse_drift_spec, BatteryState, Behavior, DriftSpec, FaultFlags,
    SeplosSim, SimReply,
};

/// Seplos RS-485 EOI byte (`\r`) that terminates every request frame.
pub const EOI: u8 = 0x0D;

/// Device id fixtures and (in networked mode) the sim itself are keyed under.
pub const DEVICE_ID: &str = "METEOR-001";

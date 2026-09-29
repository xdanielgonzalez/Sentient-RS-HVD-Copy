//! GPIO-driven RS-485 DE/RE via the `gpiocdev` crate (Linux GPIO character-device uAPI,
//! `/dev/gpiochipN`) — for `DirectionMode::GpioDe`, RS-485 HATs that wire the transceiver's
//! driver-enable to a spare GPIO line instead of to the UART's own RTS signal.
//!
//! No raw syscalls here — `gpiocdev` is a safe wrapper over the kernel GPIO uAPI, so (unlike
//! `rs485_ioctl.rs`) nothing in this module is `unsafe`.
//!
//! VERIFIED (`cargo build -p lib-io --features host`, `gpiocdev` 0.7.3): the API surface below
//! (`Request::builder`, `.on_chip`/`.with_consumer`/`.with_line`/`.as_output`/`.as_active_low`/
//! `.request`, `Request::set_value`, `line::Value`) compiles as written. Note `Builder`'s methods
//! mutate `&mut self` and return `&mut Self` (NOT an owned-`Self` chain) — `open_line` below calls
//! them without reassigning `builder =`.
//!
//! NOT verified (no `/dev/gpiochipN` in this sandbox/CI container — see `tests/direction_mode.rs`'s
//! module doc): that a real GPIO request/set-value round-trip actually drives a physical line, or
//! that `settle_tx_us`/`settle_rx_us`'s defaults are adequate transceiver timing. **Field task:
//! verify both on the shipping HAT** before enabling `DirectionMode::GpioDe` in production config.

use std::io;

use gpiocdev::line::Value;
use gpiocdev::Request;

/// Resolve a configured chip identifier to a device path: accepts either the bare chip name
/// (`"gpiochip4"`, the form named in this workstream's task/config docs) or an already-full device
/// path (`"/dev/gpiochip4"`).
pub(super) fn chip_path(chip: &str) -> String {
    if chip.starts_with('/') {
        chip.to_string()
    } else {
        format!("/dev/{chip}")
    }
}

/// Request `line` on `chip` as an output, initially DEASSERTED (RS-485 DE off / receive-only) —
/// never start a port with the bus driver-enabled and nothing yet queued to send. Held open for
/// the lifetime of the port (see `Rs485::gpio_line`'s doc comment: dropping the returned
/// [`Request`] releases the line back to the kernel's default/tristate).
///
/// `active_high` is encoded once here via `gpiocdev`'s own active-low line config, NOT re-checked
/// on every [`set`] call — [`set`]'s "asserted" always maps to [`Value::Active`], and `gpiocdev`
/// translates that through the line's configured polarity to the actual electrical level.
pub(super) fn open_line(chip: &str, line: u32, active_high: bool) -> io::Result<Request> {
    let mut builder = Request::builder();
    builder
        .on_chip(chip_path(chip))
        .with_consumer("sentient-rs-rs485-de")
        .with_line(line)
        .as_output(Value::Inactive);
    if !active_high {
        builder.as_active_low();
    }
    builder.request().map_err(io::Error::other)
}

/// Assert (`true`) or deassert (`false`) DE on the already-open `req`'s `line`.
pub(super) fn set(req: &Request, line: u32, asserted: bool) -> io::Result<()> {
    let value = if asserted {
        Value::Active
    } else {
        Value::Inactive
    };
    req.set_value(line, value).map_err(io::Error::other)
}

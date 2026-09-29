//! Bridges the `log` facade to `defmt`/RTT — feature `defmt` only. `impl_app::run`'s poll loop
//! emits its diagnostics via `log::info!`/`log::warn!`, not `defmt` directly; with no `log::Log`
//! registered those calls are silent no-ops, which this module fixes for a `defmt` build.
//!
//! `log::Record::args()` returns `core::fmt::Arguments`, which defmt cannot format directly;
//! [`defmt::Display2Format`] wraps it (via `Display`) into something that does. The bridge
//! forwards each record's target + rendered message to the defmt macro matching its `log::Level`.
use log::{Level, Log, Metadata, Record};

/// Zero-sized `log::Log` impl — one static instance ([`LOGGER`]) is registered by `main()` via
/// `log::set_logger`, as early as possible after `embassy_rp::init`.
pub(crate) struct DefmtLogger;

/// The single global logger instance `main()` hands to `log::set_logger`.
pub(crate) static LOGGER: DefmtLogger = DefmtLogger;

impl Log for DefmtLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        // The real filter is `log::set_max_level` (set alongside `set_logger` in `main()`) — this
        // impl accepts everything the `log` crate itself already decided to forward.
        true
    }

    fn log(&self, record: &Record) {
        let target = record.target();
        let msg = defmt::Display2Format(record.args());
        match record.level() {
            Level::Error => defmt::error!("[{=str}] {}", target, msg),
            Level::Warn => defmt::warn!("[{=str}] {}", target, msg),
            Level::Info => defmt::info!("[{=str}] {}", target, msg),
            Level::Debug => defmt::debug!("[{=str}] {}", target, msg),
            Level::Trace => defmt::trace!("[{=str}] {}", target, msg),
        }
    }

    fn flush(&self) {
        // defmt-rtt has no explicit flush hook (the RTT ring buffer is drained by the debug probe
        // asynchronously) — nothing to do here.
    }
}

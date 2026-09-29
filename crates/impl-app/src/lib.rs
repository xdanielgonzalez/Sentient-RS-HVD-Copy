//! The shared adapter application.
//!
//! One poll → decode → TMS map → publish loop ([`run`]) parameterized over a [`Platform`] — the
//! per-target factory that supplies the serial transport, the publish sink, and the clock. Pure
//! policy over the `Platform` seam (ADR 0009, `docs/decisions/0009-spec-app-ports-and-adapters.md`):
//! no platform concretion lives in this crate. The RPi platform lives in `bin-rpi-platform`, the
//! Zephyr MCU platform in `bin-zephyr-platform` (both colocated with their entry points). Either
//! way the entry points (`bin-rpi`, `bin-zephyr`) are thin: build a platform, call `run`. The
//! pipeline itself is identical across targets.
#![no_std]
// Platform-free shared loop — no FFI anywhere in this crate (the Zephyr FFI adapters live in
// bin-zephyr-platform; the RPi tokio/broker adapter lives in bin-rpi-platform).
#![forbid(unsafe_code)]

use lib_io::driver::{read_fault, read_telemetry};
use lib_mapping::sink::MAX_PAYLOAD;
use lib_parse::seplos::Seplos;

// The RS-232 Voltronic inverter leg (decision 0001) is a Pi-only concern: its poll loop
// ([`run_inverter`]/[`run_cycle_inverter`]) and the `Voltronic` parser it drives are compiled only
// under the `inverter` feature, so the MCU no_std build never needs `lib-parse/voltronic` and stays
// unchanged (the `Platform::emit_inverter_*` seam is a defaulted no-op there). See D9.
#[cfg(feature = "inverter")]
use lib_io::driver::{
    read_inverter_fault, read_inverter_mode, read_inverter_ratings, read_inverter_serial,
    read_inverter_status,
};
#[cfg(feature = "inverter")]
use lib_parse::canonical::{InverterMode, InverterRatings};
#[cfg(feature = "inverter")]
use lib_parse::voltronic::Voltronic;

mod impl_backoff;
mod types;

pub use types::*;

// Path-stability re-exports (ADR 0009, WS1): `Platform`/`RunConfig`/`Health`/`MAX_DIPS`/
// `UNHEALTHY_AFTER_CYCLES`/`preflight` moved into `spec-app` (the contract crate). Every existing
// `impl_app::*` path keeps resolving unchanged for bin-rpi, bin-zephyr-platform, bin-rpi-platform,
// tests, and the west-built bin/zephyr app (deliberately untouched by this refactor).
pub use spec_app::preflight;
pub use spec_app::{Health, Platform, RunConfig, MAX_DIPS, UNHEALTHY_AFTER_CYCLES};

/// Consecutive failed cycles after which the loop attempts [`Platform::reconnect`].
pub const RECONNECT_AFTER_CYCLES: u32 = 3;
/// Emit a counter summary to the log every this many cycles.
pub const SUMMARY_EVERY_CYCLES: u32 = 30;

/// Run the Seplos → TMS → MQTT poll loop over `platform` forever. Every fault degrades gracefully
/// (logged + skipped); a device that goes silent times out (see the transport read deadline) and a
/// whole dead cycle backs the loop off (1 s → `cfg.max_backoff_ms`, e.g. 30 s on the Pi) instead
/// of spinning. Identical on every target — only the `Platform` (and the configured backoff
/// ceiling) differs.
///
/// The per-cycle work lives in [`run_cycle`] (testable, returns a [`CycleOutcome`]); this outer
/// loop owns the cadence, backoff, watchdog hook, and periodic health summary.
pub async fn run<P: Platform>(mut platform: P, cfg: RunConfig) -> ! {
    let vendor = Seplos::new();
    let mut buf = [0u8; 256];
    // One reused JSON-serialize scratch buffer for every publish this loop ever makes (T3
    // RAM-budget fix — see `lib_mapping::sink::publish_tms_into`'s doc comment): materialized once
    // here, in `run`'s outermost frame (which persists for the process lifetime, never re-entered
    // per cycle), instead of `publish_tms` allocating a fresh `[0u8; MAX_PAYLOAD]` on every nested
    // call — `run_cycle` can call the publish path up to 3x in a single cycle activation.
    let mut payload_buf = [0u8; MAX_PAYLOAD];
    let mut cycle: u32 = 0;
    let mut health = Health::default();
    let mut backoff = Backoff::with_max(cfg.max_backoff_ms);

    log::info!(
        "poll loop started ({} dips, {} ms)",
        cfg.dip_addresses.len(),
        cfg.interval_ms
    );
    loop {
        let outcome = run_cycle(
            &mut platform,
            &cfg,
            &vendor,
            &mut buf,
            &mut payload_buf,
            cycle,
            &mut health,
        )
        .await;

        platform.on_cycle(&health);
        if health.cycles.is_multiple_of(SUMMARY_EVERY_CYCLES) {
            log::info!(
                "health: cycles={} tlm ok/err={}/{} flt ok/err={}/{} pub ok/err={}/{} fail_streak={}",
                health.cycles, health.telemetry_ok, health.telemetry_err, health.fault_ok,
                health.fault_err, health.publish_ok, health.publish_err,
                health.consecutive_failed_cycles,
            );
        }

        cycle = cycle.wrapping_add(1);
        if outcome.any_success {
            backoff.reset();
            platform.sleep(cfg.interval_ms).await;
        } else {
            let delay = backoff.next_delay();
            log::warn!("dead cycle (no successful exchange); backing off {delay} ms");
            platform.sleep(delay).await;
            // After a sustained failure streak, try to re-establish the transport (device loss).
            if health.consecutive_failed_cycles >= RECONNECT_AFTER_CYCLES {
                let reconnected = platform.reconnect().await;
                log::warn!(
                    "reconnect attempt after {} failed cycles: {}",
                    health.consecutive_failed_cycles,
                    if reconnected { "ok" } else { "still down" },
                );
            }
        }
    }
}

/// One poll cycle: telemetry for every DIP, faults on the `fault_every` cadence, publishing each
/// mapped TMS payload. Records every outcome into `health` and reports whether anything succeeded.
///
/// Split out of [`run`] so the loop body is testable in isolation (the outer loop returns `!` and
/// cannot be). Publish failures are counted and logged, never dropped or fatal.
pub async fn run_cycle<P: Platform>(
    platform: &mut P,
    cfg: &RunConfig,
    vendor: &Seplos,
    buf: &mut [u8],
    payload_buf: &mut [u8],
    cycle: u32,
    health: &mut Health,
) -> CycleOutcome {
    let fault_due = cfg.fault_every > 0 && cycle.is_multiple_of(cfg.fault_every);
    let mut any_success = false;

    for &dip in cfg.dip_addresses.iter() {
        // Telemetry every cycle.
        match read_telemetry(platform.transport(), vendor, dip, buf).await {
            Ok(reading) => {
                any_success = true;
                health.record_telemetry(true);
                let t = platform.now();
                // Hand the compact reading to the platform's emit seam (see `Platform::emit_reading`'s
                // doc comment) — building TMS structs and serializing is entirely the platform's
                // concern from here, not the shared loop's.
                platform.emit_reading(cfg, &reading, t, payload_buf, health);
                log::debug!("published telemetry dip={dip}");
            }
            Err(_e) => {
                health.record_telemetry(false);
                log::warn!("telemetry dip={dip} skipped (transport/parse fault)");
                // T3 DIAGNOSTIC (temporary — remove once transport-vs-parse is confirmed).
                log::warn!("telemetry diag dip={dip} err={:?}", _e);
            }
        }
        // Fault flags every Nth cycle.
        if fault_due {
            match read_fault(platform.transport(), vendor, dip, buf).await {
                Ok(fault) => {
                    any_success = true;
                    health.record_fault(true);
                    let t = platform.now();
                    platform.emit_fault(cfg, &fault, t, payload_buf, health);
                }
                Err(_e) => {
                    health.record_fault(false);
                    log::warn!("fault dip={dip} skipped (transport/parse fault)");
                }
            }
        }
    }

    health.end_cycle(any_success);
    CycleOutcome { any_success }
}

/// Run the Voltronic **inverter** → TMS → MQTT poll loop over `platform` forever (D9) — the RS-232
/// leg's counterpart to [`run`]. Host-only (Pi): the MCU never polls an inverter (see the
/// module-level cfg note). Startup polls `QID` (serial, up to 3× / 2 s, informational) and `QPIRI`
/// (ratings); then loops [`run_cycle_inverter`], which per cycle polls `QPIGS` (+ merged `QMOD`
/// mode + held `QPIRI` ratings) and, on the `fault_every` cadence, `QPIWS`. Same cadence/backoff/
/// watchdog/reconnect shape as [`run`].
#[cfg(feature = "inverter")]
pub async fn run_inverter<P: Platform>(mut platform: P, cfg: RunConfig) -> ! {
    let vendor = Voltronic::new();
    let mut buf = [0u8; 256];
    // One reused serialize scratch for every publish this loop makes (same T3 RAM-budget discipline
    // as [`run`] — materialized once in the outermost frame, never per cycle).
    let mut payload_buf = [0u8; MAX_PAYLOAD];
    let mut cycle: u32 = 0;
    let mut health = Health::default();
    let mut backoff = Backoff::with_max(cfg.max_backoff_ms);
    // Held across cycles: QPIRI ratings (retried until success — see `run_cycle_inverter`).
    let mut ratings: Option<InverterRatings> = None;

    log::info!("inverter poll loop started ({} ms)", cfg.interval_ms);

    // Startup: QID serial (informational; up to 3× spaced 2 s) then a first QPIRI attempt. Both are
    // best-effort — a failure never blocks the poll loop (QPIRI is retried per cycle below).
    inverter_startup(&mut platform, &vendor, &mut buf, &mut ratings).await;

    loop {
        let outcome = run_cycle_inverter(
            &mut platform,
            &cfg,
            &vendor,
            &mut buf,
            &mut payload_buf,
            cycle,
            &mut health,
            &mut ratings,
        )
        .await;

        platform.on_cycle(&health);
        if health.cycles.is_multiple_of(SUMMARY_EVERY_CYCLES) {
            log::info!(
                "inverter health: cycles={} tlm ok/err={}/{} flt ok/err={}/{} pub ok/err={}/{} fail_streak={}",
                health.cycles, health.telemetry_ok, health.telemetry_err, health.fault_ok,
                health.fault_err, health.publish_ok, health.publish_err,
                health.consecutive_failed_cycles,
            );
        }

        cycle = cycle.wrapping_add(1);
        if outcome.any_success {
            backoff.reset();
            platform.sleep(cfg.interval_ms).await;
        } else {
            let delay = backoff.next_delay();
            log::warn!("inverter dead cycle (no successful exchange); backing off {delay} ms");
            platform.sleep(delay).await;
            if health.consecutive_failed_cycles >= RECONNECT_AFTER_CYCLES {
                let reconnected = platform.reconnect().await;
                log::warn!(
                    "inverter reconnect attempt after {} failed cycles: {}",
                    health.consecutive_failed_cycles,
                    if reconnected { "ok" } else { "still down" },
                );
            }
        }
    }
}

/// Startup phase for [`run_inverter`]: poll `QID` (serial number, up to 3× spaced 2 s — purely
/// informational, logged) then attempt `QPIRI` once (ratings). Both best-effort; a `QPIRI` miss here
/// is retried every cycle by [`run_cycle_inverter`] until it succeeds.
#[cfg(feature = "inverter")]
async fn inverter_startup<P: Platform>(
    platform: &mut P,
    vendor: &Voltronic,
    buf: &mut [u8],
    ratings: &mut Option<InverterRatings>,
) {
    let mut serial = heapless::String::<32>::new();
    for attempt in 0..3u32 {
        match read_inverter_serial(platform.transport(), vendor, buf, &mut serial).await {
            Ok(()) => {
                log::info!("inverter serial (QID): {}", serial.as_str());
                break;
            }
            Err(_e) => {
                log::warn!(
                    "inverter QID attempt {} failed (transport/parse)",
                    attempt + 1
                );
                platform.sleep(2000).await;
            }
        }
    }
    match read_inverter_ratings(platform.transport(), vendor, buf).await {
        Ok(r) => *ratings = Some(r),
        Err(_e) => log::warn!("inverter QPIRI (ratings) not yet available; will retry per cycle"),
    }
}

/// One inverter poll cycle (D9): retry `QPIRI` if ratings are not yet held; poll `QPIGS` status +
/// merge a `QMOD` mode poll and the held `QPIRI` ratings into one composite
/// [`lib_parse::CanonicalInverterReading`] (the composite-merge WS-B deferred); on the `fault_every` cadence
/// poll `QPIWS` and emit the fault (threading the last-seen mode for the synthesized mode-fault DTC —
/// see [`Platform::emit_inverter_fault`]). Every outcome is recorded into `health`; returns whether
/// anything succeeded. Split out of [`run_inverter`] so it is testable in isolation (the outer loop
/// returns `!`).
#[cfg(feature = "inverter")]
#[allow(clippy::too_many_arguments)]
pub async fn run_cycle_inverter<P: Platform>(
    platform: &mut P,
    cfg: &RunConfig,
    vendor: &Voltronic,
    buf: &mut [u8],
    payload_buf: &mut [u8],
    cycle: u32,
    health: &mut Health,
    ratings: &mut Option<InverterRatings>,
) -> CycleOutcome {
    let fault_due = cfg.fault_every > 0 && cycle.is_multiple_of(cfg.fault_every);
    let mut any_success = false;
    // Last-seen QMOD mode, for the fault emit's synthesized mode-fault DTC (defaults Unknown when
    // the status/mode poll did not succeed this cycle).
    let mut mode = InverterMode::Unknown;

    // QPIRI is retried each cycle until it succeeds; the result is held across cycles.
    if ratings.is_none() {
        match read_inverter_ratings(platform.transport(), vendor, buf).await {
            Ok(r) => *ratings = Some(r),
            Err(_e) => log::debug!("inverter QPIRI still unavailable; retrying next cycle"),
        }
    }

    // QPIGS status every cycle, merged with a QMOD mode poll + the held QPIRI ratings.
    match read_inverter_status(platform.transport(), vendor, buf).await {
        Ok(mut reading) => {
            any_success = true;
            health.record_telemetry(true);
            // Merge QMOD mode (best-effort — a mode-poll miss leaves the parser's Unknown default).
            if let Ok(m) = read_inverter_mode(platform.transport(), vendor, buf).await {
                reading.mode = m;
            }
            mode = reading.mode;
            reading.ratings = ratings.clone();
            let t = platform.now();
            platform.emit_inverter_reading(cfg, &reading, t, payload_buf, health);
            log::debug!("published inverter telemetry");
        }
        Err(_e) => {
            health.record_telemetry(false);
            log::warn!("inverter telemetry skipped (transport/parse fault)");
        }
    }

    // QPIWS fault flags every Nth cycle.
    if fault_due {
        match read_inverter_fault(platform.transport(), vendor, buf).await {
            Ok(fault) => {
                any_success = true;
                health.record_fault(true);
                let t = platform.now();
                platform.emit_inverter_fault(cfg, &fault, mode, t, payload_buf, health);
            }
            Err(_e) => {
                health.record_fault(false);
                log::warn!("inverter fault skipped (transport/parse fault)");
            }
        }
    }

    health.end_cycle(any_success);
    CycleOutcome { any_success }
}

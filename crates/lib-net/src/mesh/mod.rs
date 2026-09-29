//! `lib-mesh` — Meshtastic mesh-radio secondary publish target.
//!
//! **Codec (feature `mesh`, no_std, no alloc — both targets).** Greedily condenses a
//! [`lib_parse::canonical::CanonicalReading`] (optionally tagged with a
//! [`lib_parse::canonical::CanonicalFault`], #387, and — only when `locate` is also enabled — a
//! [`crate::locate::LocationFix`]) into a bounded on-air payload sized for a LoRa mesh frame (the
//! baseline's ~200-byte greedy condensation algorithm — see
//! `docs/gap-analysis-meteor-parity.md`'s "Meshtastic mesh-radio forwarding" finding), plus the
//! hand-rolled Meshtastic on-air protobuf + stream framing (`impl_frame`, #386's `MeshPacket.to`
//! fix). Zero `alloc`: [`condense`] writes directly into a caller-supplied `&mut [u8]`, and the
//! frame encoder does the same. This is what lets the MCU (WP3) take this feature directly.
//!
//! **Host radio loop (feature `mesh-tokio`, std/tokio — the RPi deployment target).** [`run_radio`]
//! opens the Meshtastic radio's serial port and drains condensed frames onto it; hands it to the
//! radio task over a bounded channel so a busy/absent radio never blocks the poll loop. The Pi
//! rides the SAME no_std codec as the MCU (decision: one codec, not a divergent std copy) — this
//! module is just the tokio-shaped wrapper around it.

mod prelude;
mod types;

mod impl_bounded_writer;
mod impl_condense;
mod impl_frame;
#[cfg(feature = "mesh-tokio")]
mod impl_radio_handle;
#[cfg(feature = "mesh-tokio")]
mod impl_radio_task;

pub use impl_frame::{MAX_RADIO_FRAME, PORTNUM_PRIVATE_APP};
pub use types::*;

#[cfg(feature = "mesh-tokio")]
use tokio::io::AsyncWriteExt;
#[cfg(feature = "mesh-tokio")]
use tokio::sync::mpsc::Receiver;
#[cfg(feature = "mesh-tokio")]
use tokio::sync::watch;
#[cfg(feature = "mesh-tokio")]
use tokio::time::{sleep, Duration, Instant};

/// Greedily condense `input` (reading + optional fault + optional location) into `out`, writing
/// at most `max` bytes (further bounded by `out.len()` if that's smaller). Returns the number of
/// bytes actually written — always `<= max.min(out.len())` and always the exact length of a
/// well-formed JSON object (`out[..n]` is valid UTF-8 and starts/ends with `{`/`}`).
///
/// Greedy = candidate fields are considered in the fixed priority order documented on
/// [`impl_condense::write_fields`] (SOC, pack voltage, current, min/max cell, temperatures, fault
/// bitmask, location). Each field is tried independently: one that doesn't fit the remaining
/// budget is simply skipped — it does NOT stop lower-priority fields after it from being tried
/// (fixed divergence vs. an earlier revision, which stopped permanently at the first oversized
/// field; see `docs/gap-analysis-meteor-parity.md`).
///
/// no_std, no alloc: `out` is the ONLY buffer used; nothing here allocates.
pub fn condense(input: &CondenseInput<'_>, max: usize, out: &mut [u8]) -> usize {
    let cap = max.min(out.len());
    if cap < impl_condense::EMPTY_OBJECT_LEN {
        return 0;
    }
    // Reserve the last byte of `cap` for the closing '}' written below, so the greedy loop never
    // has to special-case "does the closing brace still fit" on every field attempt.
    let prefix_cap = cap - 1;
    let len = {
        let mut w = BoundedWriter::new(&mut out[..prefix_cap]);
        // '{' always fits: prefix_cap >= 1 whenever cap >= EMPTY_OBJECT_LEN (checked above).
        let _ = core::fmt::Write::write_char(&mut w, '{');
        impl_condense::write_fields(input, &mut w);
        w.len
    };
    out[len] = b'}';
    len + 1
}

/// Drive the mesh-radio task: opens `cfg.device_path`, then drains `rx` and forwards each
/// condensed frame to the radio hardware, until `shutdown` reports true or the sender is dropped.
///
/// - **Rate-limiting** (`cfg.send_interval_ms`): the radio is never keyed more often than this
///   interval. If further frames arrive from `rx` while waiting out the interval (or while the
///   port is down), only the FRESHEST queued frame is sent — older ones are superseded and
///   counted, since stale condensed telemetry isn't worth a duty-cycle-limited send slot.
/// - **Reconnect**: a failed or dropped port triggers a bounded exponential backoff retry
///   (starting at `impl_radio_task::INITIAL_BACKOFF`, capped at
///   `impl_radio_task::MAX_BACKOFF`) rather than a busy loop.
/// - **Drop counters**: frames superseded by rate-limiting are counted locally and logged
///   periodically (`RadioHandle::dropped`, tracked by the sender side in
///   `crates/bin/rpi-platform/src/impl_host_tap.rs`, covers the separate "channel was full" case —
///   this task has no handle to that counter, only `rx` itself, per the frozen `run_radio` signature).
#[cfg(feature = "mesh-tokio")]
pub async fn run_radio(
    mut rx: Receiver<Condensed>,
    cfg: MeshConfig,
    mut shutdown: watch::Receiver<bool>,
) {
    log::info!(
        "lib-mesh: run_radio started (port={}, baud={}, channel={})",
        cfg.device_path,
        cfg.baud,
        cfg.channel
    );

    let min_interval = Duration::from_millis(cfg.send_interval_ms);
    let mut backoff = impl_radio_task::INITIAL_BACKOFF;
    let mut port: Option<tokio_serial::SerialStream> = None;
    let mut last_sent: Option<Instant> = None;
    let mut rate_limited_drops: u64 = 0;
    let mut send_buf = [0u8; MAX_RADIO_FRAME];

    loop {
        if port.is_none() {
            match impl_radio_task::open_port(&cfg) {
                Ok(p) => {
                    log::info!("lib-mesh: radio port {} opened", cfg.device_path);
                    port = Some(p);
                    backoff = impl_radio_task::INITIAL_BACKOFF;
                }
                Err(e) => {
                    log::warn!(
                        "lib-mesh: failed to open radio port {} ({e}); retrying in {backoff:?}",
                        cfg.device_path
                    );
                    tokio::select! {
                        _ = shutdown.changed() => {
                            if *shutdown.borrow() {
                                log::info!("lib-mesh: run_radio stopped (shutdown while reconnecting)");
                                return;
                            }
                        }
                        () = sleep(backoff) => {}
                    }
                    backoff = (backoff * 2).min(impl_radio_task::MAX_BACKOFF);
                    continue;
                }
            }
        }

        let mut frame = tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    log::info!("lib-mesh: run_radio stopped");
                    return;
                }
                continue;
            }
            maybe = rx.recv() => {
                match maybe {
                    Some(frame) => frame,
                    None => {
                        log::info!("lib-mesh: run_radio stopped (sender dropped)");
                        return;
                    }
                }
            }
        };

        // Coalesce: drain any additional frames already queued behind this one and keep only the
        // freshest.
        while let Ok(newer) = rx.try_recv() {
            rate_limited_drops += 1;
            frame = newer;
        }

        // Rate-limit: never key the radio more often than `send_interval_ms`.
        if let Some(last) = last_sent {
            let elapsed = last.elapsed();
            if elapsed < min_interval {
                tokio::select! {
                    _ = shutdown.changed() => {
                        if *shutdown.borrow() {
                            log::info!("lib-mesh: run_radio stopped");
                            return;
                        }
                    }
                    () = sleep(min_interval - elapsed) => {}
                }
            }
        }

        let mesh_frame = MeshFrame {
            channel: cfg.channel,
            portnum: impl_frame::PORTNUM_PRIVATE_APP,
            payload: frame.payload.as_slice(),
        };
        let n = mesh_frame.to_radio_frame(&mut send_buf);
        if n == 0 {
            log::warn!(
                "lib-mesh: condensed frame ({} bytes) could not be encoded; dropped",
                frame.payload.len()
            );
            continue;
        }

        let Some(p) = port.as_mut() else { continue };
        match p.write_all(&send_buf[..n]).await {
            Ok(()) => {
                last_sent = Some(Instant::now());
            }
            Err(e) => {
                log::warn!("lib-mesh: radio write failed ({e}); reconnecting");
                port = None;
            }
        }

        if rate_limited_drops > 0 && rate_limited_drops.is_multiple_of(10) {
            log::debug!(
                "lib-mesh: {rate_limited_drops} frame(s) superseded by rate limiting so far"
            );
        }
    }
}

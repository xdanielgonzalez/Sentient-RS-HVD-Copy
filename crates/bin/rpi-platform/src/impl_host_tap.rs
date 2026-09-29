use super::prelude::*;
use lib_net::mesh::CondenseInput;
use std::sync::atomic::Ordering;
use std::time::{SystemTime, UNIX_EPOCH};

/// Wall-clock milliseconds "now", for [`LogRecord::ts_ms`]. Saturates to 0 rather than panicking
/// if the clock is somehow before the epoch (never expected on real hardware).
fn wall_clock_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

impl HostTap {
    /// Offer `event` to this tap — infallible by contract (see [`SinkSet::offer`]'s doc comment):
    /// every path here is best-effort and never propagates a failure to the poll loop. The
    /// `Disk`/`Mesh` paths are bounded `try_send`s — a full channel increments the tap's own
    /// `dropped` counter and logs once at `warn`, never backpressuring telemetry. The `Greengrass`
    /// path forwards to [`GreengrassTap::offer`], whose own broker publish is likewise
    /// logged-and-dropped on failure.
    pub fn offer(&mut self, event: &TapEvent<'_>) {
        // Every `TapEvent` field is a reference or `Copy` POD (`&str`/`&[u8]`/`ClockMonotonic`), so
        // matching the explicit deref binds each field by value (a cheap copy, not a move out of
        // the borrow) — avoids a double-reference (`&&str`) from match ergonomics on `event`
        // directly.
        match self {
            HostTap::Disk(tap) => {
                // The disk tap tails already-rendered wire payloads (topic + bytes, exactly as
                // published) — it has no use for `Reading`/`Fault` (no topic/payload to log yet).
                if let TapEvent::Published {
                    topic,
                    payload,
                    bus,
                } = *event
                {
                    let record = LogRecord {
                        ts_ms: wall_clock_ms(),
                        bus: bus.to_string(),
                        topic: topic.to_string(),
                        payload: payload.to_vec(),
                    };
                    if tap.handle.tx.try_send(record).is_err() {
                        tap.handle.dropped.fetch_add(1, Ordering::Relaxed);
                        log::warn!("disk tap: queue full on bus {bus}; record dropped");
                    }
                }
            }
            HostTap::Mesh(tap) => {
                // The mesh tap condenses the canonical BATTERY reading (not the rendered TMS JSON)
                // into a bounded on-air frame. Inverter (`InverterReading`/`InverterFault`) mesh
                // forwarding is deliberately out of scope this round (tracked #267) — the
                // `condense` codec has no inverter shape yet — so those variants (and the
                // already-rendered `Published` bytes, which this tap has no use for) fall through
                // to the no-op arm below.
                //
                // `Reading` and `Fault` share one send path (`send_mesh_frame`): `condense`'s
                // `CondenseInput::reading` is mandatory (a frame needs a reading to be worth
                // sending at all), but `TapEvent::Fault` carries no reading of its own — see
                // `MeshTap::last_reading`'s doc comment (`types/mesh_tap.rs`) for why a per-DIP
                // cache bridges that gap. This is the safety-critical half of #387: the mesh is
                // the out-of-band channel that survives a network outage, which is exactly when a
                // pack fault must get out — so a `Fault` event is condensed and sent immediately,
                // not held for the next `Reading` cycle the way `GreengrassTap`'s merge-on-next-
                // reading cache works.
                match *event {
                    TapEvent::Reading { reading, bus, .. } => {
                        {
                            let mut cache = tap
                                .last_reading
                                .lock()
                                .unwrap_or_else(|poisoned| poisoned.into_inner());
                            let _ = cache.insert(reading.device_address, reading.clone());
                        }
                        send_mesh_frame(tap, reading, None, bus);
                    }
                    TapEvent::Fault { fault, bus, .. } => {
                        let cached_reading = {
                            let cache = tap
                                .last_reading
                                .lock()
                                .unwrap_or_else(|poisoned| poisoned.into_inner());
                            cache.get(&fault.device_address).cloned()
                        };
                        match cached_reading {
                            Some(reading) => send_mesh_frame(tap, &reading, Some(fault), bus),
                            None => {
                                // Nothing to attach the fault to yet (no `Reading` has been seen
                                // for this DIP) — dropped, not queued with a missing mandatory
                                // field. Expected only in the brief window before a bus's first
                                // successful reading poll.
                                log::debug!(
                                    "mesh tap: fault for DIP {} on bus {bus} with no cached reading yet; dropped",
                                    fault.device_address
                                );
                            }
                        }
                    }
                    TapEvent::Published { .. }
                    | TapEvent::InverterReading { .. }
                    | TapEvent::InverterFault { .. } => {}
                }
            }
            HostTap::Greengrass(tap) => {
                // Scope D dispatch (gap-closure #269 — THE fix): forward every event to the
                // `GreengrassTap`, which renders the merged flat legacy battery payload for
                // `Reading` events (publishing to `telemetry/{device_type}/{device_id}`) and
                // updates its fault-merge cache for `Fault` events. This arm was a `log::trace`
                // no-op stub that never called `offer()` — so `telemetry/#` never published in the
                // shipped binary despite a complete renderer. The `enabled` gate is the runtime
                // kill-switch; `GreengrassTap::offer` re-checks it, but short-circuiting here
                // avoids the match on a disabled tap. Infallible by contract, same as every arm.
                if tap.enabled {
                    tap.offer(event);
                }
            }
            HostTap::Dds(tap) => {
                // DDS/RTPS (#324, WP6): renders XCDR2 + enqueues on `lib_dds::DdsHandle` for
                // `Reading`/`Fault`, on this tap's own cadence — see `DdsTap::offer`.
                tap.offer(event);
            }
        }
    }
}

/// Condense `reading` (+ optional `fault`, + `tap`'s current location fix, if any) into a bounded
/// mesh frame using `tap`'s configured on-air budget, then try-send it to the radio task — the
/// same drop-and-count backpressure discipline as every other `HostTap` variant's queue (see
/// [`HostTap::offer`]'s doc comment). Shared by both the `Reading` and `Fault` arms of
/// `HostTap::offer`'s `Mesh` case.
fn send_mesh_frame(
    tap: &MeshTap,
    reading: &CanonicalReading,
    fault: Option<&CanonicalFault>,
    bus: &str,
) {
    // `condense` writes into a caller-supplied `&mut [u8]` (no_std/no-alloc contract on the shared
    // codec — see `lib_net::mesh::condense`'s doc comment); this crate is std, so a heap `Vec`
    // sized to the configured budget stands in for the fixed-size buffer the MCU side uses.
    let mut out = vec![0u8; tap.max_payload];
    let loc_fix = *tap.location_rx.borrow();
    let input = CondenseInput {
        reading,
        fault,
        loc: loc_fix.as_ref(),
    };
    let n = condense(&input, tap.max_payload, &mut out);
    if n > 0 {
        // `condense` returns bytes actually written; hitting the configured budget exactly is the
        // only available truncation signal until `condense` itself reports it (see the
        // "Meshtastic mesh-radio forwarding" gap-analysis finding).
        out.truncate(n);
        let condensed = Condensed {
            payload: out,
            truncated: n >= tap.max_payload,
        };
        if tap.handle.tx.try_send(condensed).is_err() {
            tap.handle.dropped.fetch_add(1, Ordering::Relaxed);
            log::warn!("mesh tap: queue full on bus {bus}; frame dropped");
        }
    }
}

#[cfg(test)]
// Test code may unwrap/expect/panic; the workspace lint gate denies these in production logic
// only.
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use lib_net::mesh::RadioHandle;
    use std::collections::HashMap;
    use std::sync::atomic::AtomicU64;
    use std::sync::{Arc, Mutex};

    fn reading(device_address: u8) -> CanonicalReading {
        CanonicalReading {
            device_address,
            cell_voltages_mv: heapless::Vec::new(),
            temperatures_raw: heapless::Vec::new(),
            current_ca: -144,
            total_voltage_cv: 5451,
            remaining_capacity_cah: 13_953,
            typical_capacity_cah: 18_000,
            rated_capacity_cah: 18_000,
            soc_dpct: 775,
            soh_dpct: 1000,
            cycles: 2,
            port_voltage_cv: 5453,
        }
    }

    fn tripped_fault(device_address: u8) -> CanonicalFault {
        CanonicalFault {
            device_address,
            charge_overcurrent_protection: true,
            ..CanonicalFault::default()
        }
    }

    fn zero_now() -> ClockMonotonic {
        ClockMonotonic {
            seconds: 0,
            nanoseconds: 0,
        }
    }

    /// Builds a `HostTap::Mesh` plus the receiving end of its `RadioHandle` channel, so a test can
    /// assert on what (if anything) got sent to the "radio".
    fn mesh_host_tap() -> (HostTap, tokio::sync::mpsc::Receiver<Condensed>) {
        let (tx, rx) = tokio::sync::mpsc::channel(4);
        let handle = RadioHandle {
            tx,
            dropped: Arc::new(AtomicU64::new(0)),
        };
        let (_loc_tx, loc_rx) = tokio::sync::watch::channel(None);
        let tap = MeshTap {
            handle,
            max_payload: 200,
            last_reading: Arc::new(Mutex::new(HashMap::new())),
            location_rx: loc_rx,
        };
        (HostTap::Mesh(tap), rx)
    }

    #[test]
    fn reading_then_fault_forwards_a_condensed_fault_frame_immediately() {
        let (mut tap, mut rx) = mesh_host_tap();
        let dip1_reading = reading(1);
        tap.offer(&TapEvent::Reading {
            reading: &dip1_reading,
            now: zero_now(),
            bus: "bus0",
        });
        // The `Reading` itself condenses to a frame — drain it before asserting on the fault.
        rx.try_recv().expect("reading should have produced a frame");

        let dip1_fault = tripped_fault(1);
        tap.offer(&TapEvent::Fault {
            fault: &dip1_fault,
            now: zero_now(),
            bus: "bus0",
        });
        let condensed = rx
            .try_recv()
            .expect("#387: a fault for a DIP with a cached reading must be forwarded immediately");
        let payload = core::str::from_utf8(&condensed.payload).expect("valid utf8");
        assert!(
            payload.contains("\"f\":"),
            "condensed fault frame missing the fault bitmask field: {payload}"
        );
    }

    #[test]
    fn fault_with_no_prior_reading_for_that_dip_is_dropped() {
        let (mut tap, mut rx) = mesh_host_tap();
        let fault = tripped_fault(1);
        tap.offer(&TapEvent::Fault {
            fault: &fault,
            now: zero_now(),
            bus: "bus0",
        });
        assert!(
            rx.try_recv().is_err(),
            "a fault with nothing cached to attach it to must not be sent (condense's `reading` \
             field is mandatory)"
        );
    }

    #[test]
    fn fault_for_a_different_dip_than_the_cached_reading_is_dropped() {
        let (mut tap, mut rx) = mesh_host_tap();
        let dip1_reading = reading(1);
        tap.offer(&TapEvent::Reading {
            reading: &dip1_reading,
            now: zero_now(),
            bus: "bus0",
        });
        rx.try_recv().expect("reading should have produced a frame");

        let dip2_fault = tripped_fault(2);
        tap.offer(&TapEvent::Fault {
            fault: &dip2_fault,
            now: zero_now(),
            bus: "bus0",
        });
        assert!(
            rx.try_recv().is_err(),
            "a fault for a DIP with no cached reading of its own must not borrow another DIP's \
             reading"
        );
    }
}

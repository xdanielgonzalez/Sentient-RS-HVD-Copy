use super::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

impl DdsTap {
    /// Build a `DdsTap` from a freshly minted [`lib_dds::DdsHandle`] clone and this bus's derived
    /// identity — a fresh [`EmitCadence`] (own gate state, independent of the MQTT path; see the
    /// struct doc comment).
    pub fn new(handle: DdsHandle, device_id: Identity) -> Self {
        DdsTap {
            handle,
            device_id,
            cadence: Arc::new(Mutex::new(EmitCadence::default())),
            dropped: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Offer `event`. Infallible by contract, same as every other `HostTap` variant (see
    /// `SinkSet::offer`'s doc comment): a render failure is logged and dropped; an enqueue that
    /// finds `handle`'s bounded queue full is dropped-and-counted, never blocking the poll loop.
    ///
    /// - `Reading` — render + try-send `DcMeasurementUpdate` (unconditional, mirrors
    ///   `HostPlatform::emit_reading`'s "Periodic 1 s, no gate"), then `PowerHardwareUpdate` /
    ///   `StorageUpdate` gated by this tap's own `EmitCadence::rate_allow` (at-most-once-per-10 s).
    /// - `Fault` — render + try-send `ActiveDiagnosticState`, gated by this tap's own
    ///   `EmitCadence::seplos_ads_due` (at-start + on-change + a 30 s safety net) — mirrors
    ///   `HostPlatform::emit_fault` exactly, on this tap's independent clock.
    /// - `Published`/`InverterReading`/`InverterFault` — ignored: the inverter (CONV) leg's DDS
    ///   telemetry mirrors the battery-only scope `DdsTap` covers this round (WP6); `Published` is
    ///   the already-rendered MQTT/JSON bytes, which this tap has no use for (it renders its own
    ///   XCDR2 independently, same shape as `GreengrassTap`).
    pub fn offer(&mut self, event: &TapEvent<'_>) {
        match *event {
            TapEvent::Reading { reading, now, .. } => self.offer_reading(reading, now),
            TapEvent::Fault { fault, now, .. } => self.offer_fault(fault, now),
            TapEvent::Published { .. }
            | TapEvent::InverterReading { .. }
            | TapEvent::InverterFault { .. } => {}
        }
    }

    fn offer_reading(&self, reading: &CanonicalReading, now: ClockMonotonic) {
        let mut buf = [0u8; MAX_PAYLOAD];

        // `DcMeasurementUpdate` — Periodic 1 s, unconditional (no gate; see `emit_reading`'s
        // comment on the same topic).
        self.render_and_send(TmsTopicId::DcMeasurementUpdate, &mut buf, |out| {
            render_tms_cdr(
                TmsTopic::DcMeasurementUpdate,
                self.device_id.clone(),
                reading,
                now,
                out,
            )
            .ok()
        });

        let now_d = EmitCadence::since_origin(now);
        let ph_allowed = self
            .cadence
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .rate_allow(
                topics::POWER_HARDWARE_UPDATE,
                EmitCadence::PERIODIC_10S,
                now_d,
            );
        if ph_allowed {
            self.render_and_send(TmsTopicId::PowerHardwareUpdate, &mut buf, |out| {
                render_tms_cdr(
                    TmsTopic::PowerHardwareUpdate,
                    self.device_id.clone(),
                    reading,
                    now,
                    out,
                )
                .ok()
            });
        }

        let storage_allowed = self
            .cadence
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .rate_allow(topics::STORAGE_UPDATE, EmitCadence::PERIODIC_10S, now_d);
        if storage_allowed {
            self.render_and_send(TmsTopicId::StorageUpdate, &mut buf, |out| {
                render_storage_update_cdr(self.device_id.clone(), reading, out).ok()
            });
        }
    }

    fn offer_fault(&self, fault: &CanonicalFault, now: ClockMonotonic) {
        let now_d = EmitCadence::since_origin(now);
        let due = self
            .cadence
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .seplos_ads_due(fault, now_d);
        if due {
            let mut buf = [0u8; MAX_PAYLOAD];
            self.render_and_send(TmsTopicId::ActiveDiagnosticState, &mut buf, |out| {
                render_fault_cdr(fault, self.device_id.clone(), now, out).ok()
            });
        }
    }

    /// Render via `render` into `buf`, then `try_send` a [`DdsOutbound`] for `topic` on `handle`.
    /// A render failure (buffer too small — never expected at these payload sizes) is logged and
    /// dropped; a full/closed queue is dropped-and-counted (never blocks the caller).
    fn render_and_send(
        &self,
        topic: TmsTopicId,
        buf: &mut [u8],
        render: impl FnOnce(&mut [u8]) -> Option<usize>,
    ) {
        let Some(n) = render(buf) else {
            log::warn!(
                "dds tap: XCDR2 render failed for {topic:?} (bus device {})",
                self.device_id
            );
            return;
        };
        let out = DdsOutbound {
            device_id: self.device_id.clone(),
            topic,
            payload: buf[..n].to_vec(),
        };
        if self.handle.try_send(out).is_err() {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            log::warn!("dds tap: queue full or stack gone; {topic:?} sample dropped");
        }
    }
}

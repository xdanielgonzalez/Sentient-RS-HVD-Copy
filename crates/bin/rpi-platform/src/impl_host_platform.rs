use super::prelude::*;
use lib_mapping::render::{
    render_ac_measurement_inverter, render_active_diagnostic_inverter,
    render_dc_measurement_inverter, render_power_hardware_inverter, render_storage_update,
};

impl HostPlatform {
    /// Factory: build the platform for ONE configured serial bus, given an already-minted
    /// [`BrokerPublisher`] (the embedded broker itself is owned by the caller —
    /// `bin-rpi::runtime::supervise` — and shared across every bus; see this type's doc comment),
    /// this bus's configured secondary-publish [`SinkSet`], this bus's raw wire-capture `capture`
    /// handle (`None` when `[capture]` isn't configured — see [`CaptureSerial`]'s doc comment for
    /// the resulting fast-path passthrough), and `rate_limit_period` — the resolved
    /// `TimingProfile::rate_limit_period` the caller resolved ONCE at process startup (production:
    /// 10 s; TEST-ONLY compressed in debug builds — see `crate::TimingProfile`).
    #[allow(clippy::too_many_arguments)]
    pub fn build_port(
        publisher: BrokerPublisher,
        taps: SinkSet,
        bus: String,
        port: &str,
        kind: SerialKind,
        params: SerialParams,
        direction: DirectionMode,
        read_timeout_ms: u32,
        cycle_pulse: CyclePulse,
        preflight_inputs: PreflightInputs,
        capture: Option<CaptureHandle>,
        rate_limit_period: Duration,
    ) -> anyhow::Result<Self> {
        // Serial open is non-fatal: a device whose RS-485/RS-232 port can't be opened at boot
        // (unplugged/faulty/busy, or wired to the console UART -> EPERM) must NOT crash-loop the
        // whole service and take the embedded broker down with it. Start DEGRADED instead — the
        // broker is already up (owned by the caller), `serial` becomes `Disconnected` (every I/O
        // call fails fast with `IoError::Timeout`), and the run loop's existing dead-cycle/backoff/
        // `reconnect()` path (see `Platform::reconnect` below) recovers it once the port becomes
        // available. The OpenError carries the OS detail (e.g. "No such file or directory") into
        // the log.
        let serial = match HostSerial::open_with_direction(
            port,
            kind,
            params,
            read_timeout_ms,
            direction.clone(),
        ) {
            Ok(s) => s,
            Err(e) => {
                log::warn!(
                    "serial open {port} (bus {bus}) failed: {e}; starting DEGRADED — retrying"
                );
                HostSerial::disconnected()
            }
        };
        Ok(HostPlatform {
            publisher,
            taps,
            serial: CaptureSerial {
                inner: serial,
                handle: capture.clone(),
                pending_tx: Vec::new(),
            },
            start: Instant::now(),
            bus,
            direction,
            cycle_pulse,
            port: port.to_string(),
            kind,
            params,
            read_timeout_ms,
            capture,
            preflight_inputs,
            cadence: EmitCadence::default(),
            rate_limit_period,
        })
    }
}

impl Platform for HostPlatform {
    type Transport = CaptureSerial;
    type Pub = BrokerPublisher;

    fn transport(&mut self) -> &mut CaptureSerial {
        &mut self.serial
    }
    fn publisher(&mut self) -> &mut BrokerPublisher {
        &mut self.publisher
    }
    fn now(&self) -> ClockMonotonic {
        let d = self.start.elapsed();
        ClockMonotonic {
            seconds: d.as_secs() as u32,
            nanoseconds: d.subsec_nanos(),
        }
    }
    async fn sleep(&mut self, ms: u32) {
        tokio::time::sleep(Duration::from_millis(ms as u64)).await;
    }
    /// Render every telemetry TMS payload (`DcMeasurementUpdate`, `PowerHardwareUpdate`,
    /// `StorageUpdate`) and publish each onto the embedded broker, offering each rendered
    /// publish (and the reading itself) to this bus's configured secondary-publish taps —
    /// per-target isolation: a tap failure never affects the broker publish or any other tap (see
    /// [`SinkSet::offer`]'s doc comment). `StorageUpdate` reuses the same `EnergyStorageState`
    /// construction as `PowerHardwareUpdate` via the [`ToStorageUpdate`] stub trait — see its doc
    /// comment (PROVISIONAL field mapping, gap-closure #237).
    ///
    /// **One wire format**: `DcMeasurementUpdate`/`PowerHardwareUpdate` route through the same
    /// [`lib_mapping::wire`] flat serializer the device-hosted broker's
    /// `bin_zephyr_platform::render_pending_topic` uses (via the same `render_tms`/`render_fault`
    /// seam), so host and device emit byte-identical JSON for the same canonical input.
    /// `StorageUpdate` is a plain `serde`-encoded payload (via [`publish_tms_into`]) since it has
    /// no `render_tms` variant yet — the TMS fleet agent owns adding one.
    #[cfg_attr(feature = "hotpath", hotpath::measure)]
    fn emit_reading(
        &mut self,
        cfg: &RunConfig,
        reading: &CanonicalReading,
        now: ClockMonotonic,
        payload_buf: &mut [u8],
        health: &mut Health,
    ) {
        // `DcMeasurementUpdate` is spec Periodic 1 s — published every poll cycle (no gate).
        publish_rendered_tapped(
            self,
            health,
            &cfg.prefix,
            &cfg.device_id,
            topics::DC_MEASUREMENT_UPDATE,
            payload_buf,
            |buf| {
                render_tms(
                    TmsTopic::DcMeasurementUpdate,
                    cfg.device_id.clone(),
                    reading,
                    now,
                    buf,
                )
                .ok()
            },
        );
        // `PowerHardwareUpdate` + `StorageUpdate` are spec Slow / Periodic 10 s with a "shall not
        // send faster" rate limit (IDL 216-234; §4.9.10.2). The poll loop runs far faster (~1 s),
        // so gate each to at-most-once-per `rate_limit_period` (production 10 s; TEST-ONLY
        // compressed in debug builds — see `TimingProfile`). The secondary-publish taps below are
        // UNAFFECTED (they key off `TapEvent::Reading`, offered every cycle).
        let now_d = EmitCadence::since_origin(now);
        let rate_limit_period = self.rate_limit_period;
        if self
            .cadence
            .rate_allow(topics::POWER_HARDWARE_UPDATE, rate_limit_period, now_d)
        {
            publish_rendered_tapped(
                self,
                health,
                &cfg.prefix,
                &cfg.device_id,
                topics::POWER_HARDWARE_UPDATE,
                payload_buf,
                |buf| {
                    render_tms(
                        TmsTopic::PowerHardwareUpdate,
                        cfg.device_id.clone(),
                        reading,
                        now,
                        buf,
                    )
                    .ok()
                },
            );
        }
        if self
            .cadence
            .rate_allow(topics::STORAGE_UPDATE, rate_limit_period, now_d)
        {
            publish_rendered_tapped(
                self,
                health,
                &cfg.prefix,
                &cfg.device_id,
                topics::STORAGE_UPDATE,
                payload_buf,
                |buf| render_storage_update(cfg.device_id.clone(), reading, buf).ok(),
            );
        }

        self.taps.offer(&TapEvent::Reading {
            reading,
            now,
            bus: &self.bus,
        });
    }
    /// Render `ActiveDiagnosticState` via [`render_fault`] and publish it, offering the rendered
    /// publish (and the fault itself) to this bus's configured secondary-publish taps. See
    /// [`HostPlatform::emit_reading`]'s doc comment — same seam, same per-target isolation.
    #[cfg_attr(feature = "hotpath", hotpath::measure)]
    fn emit_fault(
        &mut self,
        cfg: &RunConfig,
        fault: &CanonicalFault,
        now: ClockMonotonic,
        payload_buf: &mut [u8],
        health: &mut Health,
    ) {
        // `ActiveDiagnosticState` is spec At Start + On Change + PublishLast (B.12). Publish the
        // first snapshot (at-start) and thereafter only when the fault set changes, with a periodic
        // safety-net re-publish — see `EmitCadence::seplos_ads_due`. The tap offer below stays
        // UNCONDITIONAL: it feeds the fault-merge cache (D2), which must track every fault poll
        // regardless of whether the broker ADS publish was deduplicated.
        let now_d = EmitCadence::since_origin(now);
        if self.cadence.seplos_ads_due(fault, now_d) {
            publish_rendered_tapped(
                self,
                health,
                &cfg.prefix,
                &cfg.device_id,
                topics::ACTIVE_DIAGNOSTIC_STATE,
                payload_buf,
                |buf| render_fault(fault, cfg.device_id.clone(), now, buf).ok(),
            );
        }
        self.taps.offer(&TapEvent::Fault {
            fault,
            now,
            bus: &self.bus,
        });
    }
    /// Render the inverter CONV-role telemetry TMS payloads (`AcMeasurementUpdate`,
    /// `DcMeasurementUpdate`, `PowerHardwareUpdate` — see [`lib_mapping::transform_inverter`], D10)
    /// and publish each onto the embedded broker, offering each rendered publish (and the reading
    /// itself, as [`TapEvent::InverterReading`]) to this bus's taps. Serializes via the flat
    /// [`lib_mapping::wire`] serializer (same as the battery path) — the inverter wire bindings
    /// now have hand-written `render_*_inverter` variants; NaN fields (unknown-but-required floats)
    /// serialize to JSON `null` via [`lib_tms::fields::FloatVal::Null`].
    fn emit_inverter_reading(
        &mut self,
        cfg: &RunConfig,
        reading: &CanonicalInverterReading,
        now: ClockMonotonic,
        payload_buf: &mut [u8],
        health: &mut Health,
    ) {
        publish_rendered_tapped(
            self,
            health,
            &cfg.prefix,
            &cfg.device_id,
            topics::AC_MEASUREMENT_UPDATE,
            payload_buf,
            |buf| render_ac_measurement_inverter(cfg.device_id.clone(), reading, now, buf).ok(),
        );
        publish_rendered_tapped(
            self,
            health,
            &cfg.prefix,
            &cfg.device_id,
            topics::DC_MEASUREMENT_UPDATE,
            payload_buf,
            |buf| render_dc_measurement_inverter(cfg.device_id.clone(), reading, now, buf).ok(),
        );
        // `PowerHardwareUpdate` is spec Slow / Periodic 10 s — rate-limit it (the two
        // `MeasurementUpdate`s above are Periodic 1 s and publish every cycle). See
        // `HostPlatform::emit_reading`'s comment on `rate_limit_period` above.
        let now_d = EmitCadence::since_origin(now);
        let rate_limit_period = self.rate_limit_period;
        if self
            .cadence
            .rate_allow(topics::POWER_HARDWARE_UPDATE, rate_limit_period, now_d)
        {
            publish_rendered_tapped(
                self,
                health,
                &cfg.prefix,
                &cfg.device_id,
                topics::POWER_HARDWARE_UPDATE,
                payload_buf,
                |buf| render_power_hardware_inverter(cfg.device_id.clone(), reading, now, buf).ok(),
            );
        }
        self.taps.offer(&TapEvent::InverterReading {
            reading,
            now,
            bus: &self.bus,
        });
    }
    /// Render `ActiveDiagnosticState` from the inverter QPIWS fault flags plus the QMOD `mode` (the
    /// mode word's only TMS home is a synthesized DTC folded in here — see
    /// [`lib_mapping::transform_inverter::to_active_diagnostics`], D10) and publish it, offering the
    /// rendered publish (and the fault, as [`TapEvent::InverterFault`]) to this bus's taps.
    fn emit_inverter_fault(
        &mut self,
        cfg: &RunConfig,
        fault: &CanonicalInverterFault,
        mode: InverterMode,
        now: ClockMonotonic,
        payload_buf: &mut [u8],
        health: &mut Health,
    ) {
        // `ActiveDiagnosticState` At Start + On Change (B.12): publish the first snapshot and
        // thereafter only when the (fault set, QMOD mode) changes, with a safety-net re-publish —
        // see `EmitCadence::inverter_ads_due`. The tap offer stays unconditional (D2 fault cache).
        let now_d = EmitCadence::since_origin(now);
        if self.cadence.inverter_ads_due(fault, mode, now_d) {
            publish_rendered_tapped(
                self,
                health,
                &cfg.prefix,
                &cfg.device_id,
                topics::ACTIVE_DIAGNOSTIC_STATE,
                payload_buf,
                |buf| {
                    render_active_diagnostic_inverter(fault, mode, cfg.device_id.clone(), now, buf)
                        .ok()
                },
            );
        }
        self.taps.offer(&TapEvent::InverterFault {
            fault,
            now,
            bus: &self.bus,
        });
    }
    /// Pet this bus's [`CyclePulse`] every completed cycle — LIVENESS, not telemetry health.
    ///
    /// `on_cycle` is only reached after a full `run_cycle` returns, so reaching it at all *is* the
    /// liveness signal: a genuinely hung loop (deadlocked await, poisoned lock, runaway retry)
    /// never gets here. The actual systemd watchdog feed moved to
    /// [`crate::WatchdogAgg::run`] (aggregating one `CyclePulse` per bus/background task, run
    /// once by `bin-rpi::runtime::supervise`) — `on_cycle` itself no longer touches `sd_notify`
    /// directly, so multiple `HostPlatform`s (one per bus) never race each other's watchdog feed.
    ///
    /// Unconditional (no `health.healthy()` gate — see bug #172): a serial that starts (or stays)
    /// disconnected still reaches `on_cycle` every cycle (the DEGRADED design keeps cycling and
    /// retrying via `Platform::reconnect`), so this bus's pulse — and therefore the aggregated
    /// watchdog feed — is never starved by a telemetry-only failure.
    fn on_cycle(&mut self, _health: &Health) {
        self.cycle_pulse.pet();
    }
    /// Reopen the serial transport after a device loss (e.g. a USB adapter re-enumerated). Returns
    /// true if the port reopened. On failure the old transport is left in place and the loop keeps
    /// backing off; the aggregated systemd watchdog is the backstop if it never recovers.
    ///
    /// Re-wraps the freshly reopened [`HostSerial`] in a fresh [`CaptureSerial`], cloning `self`'s
    /// own retained `capture` handle into it — so a bus with `[capture]` configured keeps recording
    /// to the SAME writer/channel across a reconnect, rather than silently reverting to passthrough.
    async fn reconnect(&mut self) -> bool {
        match HostSerial::open_with_direction(
            &self.port,
            self.kind,
            self.params,
            self.read_timeout_ms,
            self.direction.clone(),
        ) {
            Ok(s) => {
                self.serial = CaptureSerial {
                    inner: s,
                    handle: self.capture.clone(),
                    pending_tx: Vec::new(),
                };
                log::info!("serial reconnected on {} (bus {})", self.port, self.bus);
                true
            }
            Err(e) => {
                log::warn!(
                    "serial reconnect on {} (bus {}) failed: {e}",
                    self.port,
                    self.bus
                );
                false
            }
        }
    }
}

/// Build `{prefix}/{device_id}/{slug}` for `topic`, render into `payload_buf` via `render`, offer
/// the rendered `(topic, payload)` to `platform`'s taps, THEN publish onto the broker and record
/// the outcome into `health` — **broker outcome only feeds `health`** (a tap failure is invisible
/// to `Health`/the loop's backoff by contract; see [`SinkSet::offer`]'s doc comment). Never drops
/// a broker-publish failure silently: it is logged (never fatal) and counted.
#[cfg_attr(feature = "hotpath", hotpath::measure)]
fn publish_rendered_tapped(
    platform: &mut HostPlatform,
    health: &mut Health,
    prefix: &str,
    device_id: &Identity,
    topic: &str,
    payload_buf: &mut [u8],
    render: impl FnOnce(&mut [u8]) -> Option<usize>,
) {
    let mut topic_buf = heapless::String::<MAX_TOPIC>::new();
    if topic_into(prefix, device_id, topic, &mut topic_buf).is_err() {
        health.record_publish(false);
        log::warn!("topic build failed for {topic} (bus {})", platform.bus);
        return;
    }
    let Some(n) = render(payload_buf) else {
        health.record_publish(false);
        log::warn!("render failed for {topic} (bus {})", platform.bus);
        return;
    };
    platform.taps.offer(&TapEvent::Published {
        topic: &topic_buf,
        payload: &payload_buf[..n],
        bus: &platform.bus,
    });
    match platform.publisher.publish(&topic_buf, &payload_buf[..n]) {
        Ok(()) => health.record_publish(true),
        Err(_e) => {
            health.record_publish(false);
            log::warn!(
                "publish failed on {topic} (bus {}, sink fault); continuing",
                platform.bus
            );
        }
    }
}

impl Preflight for HostPlatform {
    type Ctx = HostCtx;

    fn context(&mut self) -> HostCtx {
        HostCtx {
            serial_present: std::path::Path::new(&self.port).exists(),
            serial_label: self.port.clone(),
            config_valid: if !self.preflight_inputs.device_id_present {
                Err("device.id is empty")
            } else if !self.preflight_inputs.dip_addresses_present {
                Err("poll.dip_addresses is empty")
            } else {
                Ok(())
            },
            cloud: self.preflight_inputs.cloud,
        }
    }

    /// Run the host's ordered preflight checks. Call once, before the poll loop starts.
    async fn preflight(&mut self) -> Report {
        let ctx = self.context();
        run_checks!(ctx => SerialPresent, ConfigValid, CloudCredsPresent, MosquittoBridgeRenderable)
    }
}

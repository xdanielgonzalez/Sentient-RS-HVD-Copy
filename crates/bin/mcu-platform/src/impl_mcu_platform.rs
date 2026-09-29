use crate::prelude::*;

impl<U, D, T, P, C> Platform for McuPlatform<U, D, T, P, C>
where
    U: Read + Write,
    D: OutputPin,
    T: DelayNs,
    P: Publisher,
    C: McuClock,
{
    type Transport = EmbassyUart<U, D, T>;
    type Pub = P;

    fn transport(&mut self) -> &mut Self::Transport {
        &mut self.transport
    }
    fn publisher(&mut self) -> &mut Self::Pub {
        &mut self.publisher
    }
    fn now(&self) -> ClockMonotonic {
        self.clock.now()
    }
    async fn sleep(&mut self, ms: u32) {
        self.clock.sleep_ms(ms).await;
    }

    /// Render `DcMeasurementUpdate`/`PowerHardwareUpdate`/`StorageUpdate` via
    /// [`lib_mapping::render`] (the SAME flat wire serializer
    /// `bin_zephyr_platform::ZephyrPlatform`'s `RemoteClient` branch and
    /// `bin_rpi_platform::HostPlatform` both use) and publish each — one wire format, no new
    /// serialization code. The MCU has no arena/device-hosted-broker split (unlike the Zephyr
    /// `Broker` variant this crate supersedes), so there is only ever this one path.
    ///
    /// **`StorageUpdate` (Seplos SoC/energy — TMS topic-parity with the Pi, gap-closure).** Sourced
    /// straight from the same `CanonicalReading` (real Seplos data, NOT a static declaration), via
    /// [`render_storage_update`] — the identical call `bin_rpi_platform::HostPlatform::emit_reading`
    /// makes. Published every poll cycle here (no rate limiter): the MCU has no `EmitCadence`, and
    /// deliberately publishes `PowerHardwareUpdate` every cycle too — see `ns::run_config`'s doc
    /// comment for why the MCU's telemetry topics are un-gated on this path (same defect class as the
    /// Pi's shipped configs; the poll interval itself is the rate).
    fn emit_reading(
        &mut self,
        cfg: &RunConfig,
        reading: &CanonicalReading,
        now: ClockMonotonic,
        payload_buf: &mut [u8],
        health: &mut Health,
    ) {
        remote_publish_rendered(
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
        remote_publish_rendered(
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
        remote_publish_rendered(
            self,
            health,
            &cfg.prefix,
            &cfg.device_id,
            topics::STORAGE_UPDATE,
            payload_buf,
            |buf| render_storage_update(cfg.device_id.clone(), reading, buf).ok(),
        );
    }

    /// Render `ActiveDiagnosticState` via [`lib_mapping::render::render_fault`] and publish it. See
    /// [`emit_reading`](Platform::emit_reading)'s doc comment — same seam, same one-wire-format
    /// reuse.
    fn emit_fault(
        &mut self,
        cfg: &RunConfig,
        fault: &CanonicalFault,
        now: ClockMonotonic,
        payload_buf: &mut [u8],
        health: &mut Health,
    ) {
        remote_publish_rendered(
            self,
            health,
            &cfg.prefix,
            &cfg.device_id,
            topics::ACTIVE_DIAGNOSTIC_STATE,
            payload_buf,
            |buf| render_fault(fault, cfg.device_id.clone(), now, buf).ok(),
        );
    }

    // `emit_inverter_reading`/`emit_inverter_fault`/`on_cycle`/`reconnect` all inherit
    // `Platform`'s default no-ops — the MCU never polls an inverter (same doctrine as
    // `bin_zephyr_platform::ZephyrPlatform`) and has no watchdog/reopen story of its own yet.

    /// Feature `power` only — absent otherwise, so `Platform`'s default no-op is inherited
    /// unchanged when this feature is off. Thin wrapper over [`publish_rail_draw`] (below) so the
    /// SAME render/publish logic is reachable two ways: through this trait method (any generic
    /// `P: Platform` caller), and directly (no full `McuPlatform` needed) by
    /// `ns::boot_i2c::rail_task`, which polls the INA3221 on its own independent `Timer`-driven
    /// cadence — deliberately NOT through the Seplos poll cycle this impl's other `emit_*` methods
    /// ride (rail polling must never be gated on `POLL_HEARTBEAT`/the parser loop, so a stalled
    /// I2C read can't affect the watchdog).
    ///
    /// **This is diagnostics data, not TMS mission telemetry** — see
    /// [`spec_app::Platform::emit_rail_draw`]'s doc comment. `cfg`/`now` are unused: the
    /// diagnostic payload carries no device-id/topic-prefix/timestamp fields (unlike the TMS
    /// publishes above), only the two rail currents.
    #[cfg(feature = "power")]
    fn emit_rail_draw(
        &mut self,
        _cfg: &RunConfig,
        readings: &[DeviceRailReading],
        _now: ClockMonotonic,
        payload_buf: &mut [u8],
        health: &mut Health,
    ) {
        publish_rail_draw(&mut self.publisher, health, readings, payload_buf);
    }
}

/// Render a batch of INA3221 channel readings as a small, non-TMS diagnostic JSON payload
/// (`{"eth_ma":150,"eth_mv":5480,"radio_ma":276,"radio_mv":5490,"mcu_ma":40,"mcu_mv":3300}`) and
/// publish it to the fixed `diag/rails` topic through
/// `publisher`, recording the outcome into `health`. Deliberately calls nothing in `lib_mapping`:
/// this is adapter self-telemetry (the W5500/Heltec rails), not TMS mission data — the project's
/// power telemetry is the Seplos BMS pack, mapped in `lib_mapping::transform` via
/// [`spec_app::Platform::emit_reading`]. See [`spec_app::Platform::emit_rail_draw`]'s doc comment
/// for the correction this fn's shape records.
///
/// Free fn (not a method on `McuPlatform`) so a caller that polls the INA3221 independently of the
/// Seplos poll cycle — `ns::boot_i2c::rail_task` — can call it directly with its own `Publisher`
/// handle (a second `ChannelPublisher` onto the same `MQTT_CHANNEL`), without needing a full
/// `McuPlatform` (whose `transport`/`clock` fields that leg never touches).
/// [`McuPlatform::emit_rail_draw`] above is a thin wrapper over this SAME fn for the
/// `Platform`-trait-generic caller — one render/publish implementation, two call sites.
#[cfg(feature = "power")]
pub fn publish_rail_draw<P: Publisher>(
    publisher: &mut P,
    health: &mut Health,
    readings: &[DeviceRailReading],
    payload_buf: &mut [u8],
) {
    match render_rail_draw_json(readings, payload_buf) {
        Some(n) => match publisher.publish(DIAG_RAILS_TOPIC, &payload_buf[..n]) {
            Ok(()) => health.record_publish(true),
            Err(_e) => health.record_publish(false),
        },
        None => health.record_publish(false),
    }
}

/// Fixed, clearly non-TMS topic for the rail-draw diagnostic (no `tms/...` prefix, no
/// device-id/topic-prefix templating like the TMS publishes above — this is a device-local
/// diagnostic, not mission telemetry namespaced per the TMS convention).
#[cfg(feature = "power")]
pub const DIAG_RAILS_TOPIC: &str = "diag/rails";

/// Fixed-point milliamps-per-microvolt divisor for the Adafruit INA3221 (PID 6062) breakout's
/// 0.05 Ω shunt resistors (milliamps = microvolts / 50) — PROVISIONAL, unverified against the
/// physical board (see `lib_canonical::DeviceRailReading`'s own doc comment). Local to this
/// diagnostic path; deliberately NOT shared with `lib_mapping` (which has no knowledge of rail
/// telemetry at all after this correction).
#[cfg(feature = "power")]
const SHUNT_MILLIOHMS: i32 = 50;

/// Render `readings` as a compact, flat JSON object with a current (`_ma`) AND a bus-voltage
/// (`_mv`) field per rail, keyed by rail name — e.g.
/// `{"eth_ma":150,"eth_mv":5480,"radio_ma":276,"radio_mv":5490,"mcu_ma":40,"mcu_mv":3300}`; `Spare`
/// readings are skipped — into `out`, returning the byte count written. Current is
/// `shunt_uv / SHUNT_MILLIOHMS` (Ohm's law over the fixed shunt); voltage is the INA3221's
/// `bus_mv` measurement passed through unchanged. No allocation: a stack-local `heapless::String`
/// scratch (sized for all three rails × two fields), copied into the caller's buffer. Returns
/// `None` if the rendered object would not fit `out`.
#[cfg(feature = "power")]
fn render_rail_draw_json(readings: &[DeviceRailReading], out: &mut [u8]) -> Option<usize> {
    use core::fmt::Write as _;

    // Sized for 3 rails × (`_ma` + `_mv`) fields with signed/5-digit values — ~120 chars of content
    // plus braces/commas; 160 leaves margin. Still a bounded stack local, reused per call.
    let mut buf: heapless::String<160> = heapless::String::new();
    buf.push('{').ok()?;
    let mut first = true;
    for r in readings {
        let name = match r.rail {
            DeviceRail::Ethernet => "eth",
            DeviceRail::Radio => "radio",
            DeviceRail::Mcu => "mcu",
            DeviceRail::Spare => continue,
        };
        if !first {
            buf.push(',').ok()?;
        }
        first = false;
        let ma = r.shunt_uv / SHUNT_MILLIOHMS;
        let mv = r.bus_mv;
        write!(buf, "\"{name}_ma\":{ma},\"{name}_mv\":{mv}").ok()?;
    }
    buf.push('}').ok()?;

    if buf.len() > out.len() {
        return None;
    }
    out[..buf.len()].copy_from_slice(buf.as_bytes());
    Some(buf.len())
}

/// Render + publish one TMS payload through `platform`'s publisher, recording the outcome into
/// `health`. Mirrors `bin_zephyr_platform`'s `remote_publish_rendered` / `bin_rpi_platform`'s
/// `publish_rendered_tapped` shape (minus the taps — this platform has none).
fn remote_publish_rendered<U, D, T, P: Publisher, C: McuClock>(
    platform: &mut McuPlatform<U, D, T, P, C>,
    health: &mut Health,
    prefix: &str,
    device_id: &Identity,
    topic: &str,
    payload_buf: &mut [u8],
    render: impl FnOnce(&mut [u8]) -> Option<usize>,
) {
    match publish_rendered(
        &mut platform.publisher,
        prefix,
        device_id,
        topic,
        payload_buf,
        render,
    ) {
        Ok(()) => health.record_publish(true),
        Err(_e) => health.record_publish(false),
    }
}

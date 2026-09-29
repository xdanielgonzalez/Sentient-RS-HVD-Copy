use crate::{Health, RunConfig};
use lib_io::transport::SerialTransport;
use lib_mapping::schema::{ClockMonotonic, DeviceRailReading};
use lib_mapping::sink::Publisher;
use lib_parse::canonical::{
    CanonicalFault, CanonicalInverterFault, CanonicalInverterReading, CanonicalReading,
    InverterMode,
};

/// A target platform: the factory-built transport + publish sink + clock. Concrete impls live in
/// the feature-gated `host` / `zephyr` modules.
#[allow(async_fn_in_trait)]
pub trait Platform {
    type Transport: SerialTransport;
    type Pub: Publisher;

    /// The serial transport (Seplos bus).
    fn transport(&mut self) -> &mut Self::Transport;
    /// The publish sink (embedded broker on RPi, MQTT client on Zephyr).
    fn publisher(&mut self) -> &mut Self::Pub;
    /// A monotonic timestamp for TMS payloads.
    fn now(&self) -> ClockMonotonic;
    /// Wait `ms` milliseconds between cycles (tokio timer on RPi, `k_msleep` on Zephyr).
    async fn sleep(&mut self, ms: u32);
    /// Emit one decoded telemetry reading (the platform-specific "output" seam — see the crate
    /// root docs' "MEMORY-MODEL REDESIGN" pointer). `impl_app::run_cycle` hands over the compact
    /// [`CanonicalReading`] as soon as it decodes; everything downstream (build TMS, serialize,
    /// publish — or, on a RAM-constrained target, defer all of that) is entirely the platform's
    /// concern, done here rather than inside the shared poll loop's own stack frame.
    ///
    /// `payload_buf` is `impl_app::run`'s single reused JSON-serialize scratch (sized `>= MAX_PAYLOAD`) —
    /// offered to platforms that serialize synchronously here (the host path); a platform that
    /// defers serialization (Zephyr) is free to ignore it. Every publish outcome must be recorded
    /// into `health` via [`Health::record_publish`] so cycle accounting stays correct regardless of
    /// platform.
    fn emit_reading(
        &mut self,
        cfg: &RunConfig,
        reading: &CanonicalReading,
        now: ClockMonotonic,
        payload_buf: &mut [u8],
        health: &mut Health,
    );
    /// Emit one decoded fault snapshot. See [`Platform::emit_reading`]'s doc comment — same seam,
    /// for the `fault_every`-cadence exchange.
    fn emit_fault(
        &mut self,
        cfg: &RunConfig,
        fault: &CanonicalFault,
        now: ClockMonotonic,
        payload_buf: &mut [u8],
        health: &mut Health,
    );
    /// Emit one decoded **inverter** telemetry reading (the RS-232 Voltronic leg — decision 0001).
    /// Sibling of [`Platform::emit_reading`] for the CONV-role inverter path: `impl_app::run_cycle_inverter`
    /// hands over the compact composite [`CanonicalInverterReading`] (QPIGS + merged QMOD mode +
    /// held QPIRI ratings) as soon as it decodes; building the CONV TMS structs (AC/DC/PowerHardware
    /// per `lib_mapping::transform_inverter`), serializing, and publishing is entirely the
    /// platform's concern from here.
    ///
    /// **Default: no-op.** The MCU (`bin_zephyr_platform::ZephyrPlatform`) never polls an inverter
    /// (a Pi-only leg), so it inherits this no-op and compiles unchanged — the RPi platform
    /// (`bin_rpi_platform::HostPlatform`) overrides it. `payload_buf` is the shared reused
    /// serialize scratch (`>= MAX_PAYLOAD`), same contract as [`Platform::emit_reading`].
    fn emit_inverter_reading(
        &mut self,
        _cfg: &RunConfig,
        _reading: &CanonicalInverterReading,
        _now: ClockMonotonic,
        _payload_buf: &mut [u8],
        _health: &mut Health,
    ) {
    }
    /// Emit one decoded **inverter** fault snapshot (QPIWS flags), plus `mode` — the current QMOD
    /// operating mode, threaded in because the QMOD word has no faithful TMS topic of its own and
    /// its only TMS representation is a synthesized DTC folded into this diagnostic emit (D10; see
    /// [`lib_mapping::transform_inverter::to_active_diagnostics`]). Default no-op, for the same
    /// MCU-unaffected reason as [`Platform::emit_inverter_reading`].
    fn emit_inverter_fault(
        &mut self,
        _cfg: &RunConfig,
        _fault: &CanonicalInverterFault,
        _mode: InverterMode,
        _now: ClockMonotonic,
        _payload_buf: &mut [u8],
        _health: &mut Health,
    ) {
    }
    /// Emit a batch of decoded **device-rail draw** readings (the INA3221 multi-rail leg —
    /// adapter self-telemetry, NOT the project's power source). Sibling of
    /// [`Platform::emit_inverter_reading`], same seam: whichever platform polls a
    /// [`crate::RailMonitor`] hands over the `<= MAX_DEVICE_RAILS`-length batch
    /// [`crate::RailMonitor::read_all`] just filled.
    ///
    /// **This is diagnostics/UI data, not TMS mission telemetry.** The project's power telemetry
    /// (pack voltage/current/SOC) comes from the Seplos BMS, mapped in `lib_mapping::transform`
    /// via [`Platform::emit_reading`] — an earlier wave wrongly routed this rail data onto the
    /// SAME `DcMeasurementUpdate.internal_measurement` TMS field the pack uses; that mapping was
    /// removed. An implementor of this method must publish to a clearly non-TMS topic
    /// (`bin-mcu-platform`'s override uses `diag/rails`) and must not build any TMS/`lib_mapping`
    /// struct from `readings`.
    ///
    /// **Default: no-op.** No platform polls an INA3221 today, so every existing platform
    /// (`bin_rpi_platform::HostPlatform`, `bin_zephyr_platform::ZephyrPlatform`) inherits this
    /// no-op and compiles unchanged — the platform that wires the driver overrides it.
    /// `payload_buf` is the shared reused serialize scratch (`>= MAX_PAYLOAD`), same contract
    /// as [`Platform::emit_reading`].
    fn emit_rail_draw(
        &mut self,
        _cfg: &RunConfig,
        _readings: &[DeviceRailReading],
        _now: ClockMonotonic,
        _payload_buf: &mut [u8],
        _health: &mut Health,
    ) {
    }
    /// Called once per completed cycle with the running health snapshot. Default no-op; the host
    /// platform overrides it to feed the systemd watchdog — reaching this hook is itself the
    /// liveness signal, independent of [`Health::healthy`] (telemetry success is a separate
    /// concern; a serial-degraded device must keep petting the watchdog and self-heal via
    /// `reconnect`, not be restarted by systemd).
    fn on_cycle(&mut self, _health: &Health) {}
    /// Attempt to re-establish the transport after repeated failures (e.g. reopen a serial port
    /// whose device re-enumerated). Default: not supported (returns false), so the Zephyr platform
    /// is unaffected. The host platform reopens the serial port.
    async fn reconnect(&mut self) -> bool {
        false
    }
}

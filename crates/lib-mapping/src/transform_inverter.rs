//! Canonical **inverter** reading/fault → TMS conversions (D10, PROVISIONAL).
//!
//! Sibling of [`crate::transform`] (the battery/STOR mapper): this module maps the novel
//! [`lib_canonical::CanonicalInverterReading`]/[`lib_canonical::CanonicalInverterFault`] (RS-232 Voltronic/PI30 leg — decision
//! 0001, in-scope) onto the CONV-role TMS topics. Kept `no_std`/no-alloc exactly like
//! `transform.rs` (bounded `heapless::Vec`, `f32` arithmetic only, zero I/O) so it compiles on the
//! MCU no_std target too, even though the inverter leg is a Pi-only concern today.
//!
//! **Everything here is PROVISIONAL** — every port assignment, SPN, and scale choice is derived
//! from the legacy Python reference + protocol docs, with NO real Voltronic/QPIGS wire captures
//! (decision 0001's open risk). Each mapping choice is doc-commented at its site with the reasoning
//! so a real capture can confirm or correct it. Per the standard, an unknown-but-required `f32`
//! field is `f32::NAN` (serialized as JSON `null` by the no_std codec — never an invented value),
//! and an `@optional` field with no source is `None`.
//!
//! ## Topic mapping (CONV role, MIL-STD-3071)
//! * `AcMeasurementUpdate` — two external AC ports: the load-side AC output (port 0) and the
//!   utility grid input (port 1). QPIGS carries neither grid current nor grid power, so the grid
//!   port's amperage/real/reactive power are `NaN`; the AC-output port derives amperage from
//!   apparent-power ÷ voltage (the only current QPIGS implies) and carries the reported active
//!   power.
//! * `DcMeasurementUpdate` — two external DC ports: the PV/solar input (port 2) and the battery DC
//!   bus (port 3). Battery amperage uses the source-positive TMS convention (discharge − charge).
//! * `PowerHardwareUpdate` — only `power_electronics.temperature` (the inverter heatsink temp);
//!   the other five `@optional` subsystem states have no inverter source and stay `None`.
//! * `ActiveDiagnosticState` — one `Major`/`ConditionExists` DTC per set QPIWS bit (SPN = 100 +
//!   QPIWS bit index, disjoint from the battery's SPN 1..16), plus a synthesized SPN 130 DTC when
//!   the QMOD mode is `Fault` (the mode word has no faithful TMS home of its own — see D10).

use heapless::Vec;
#[allow(unused_imports)]
use lib_parse::canonical::InverterMode;
use lib_parse::canonical::{CanonicalInverterFault, CanonicalInverterReading};
#[allow(unused_imports)]
use lib_tms::{
    AcMeasurementUpdate, AcPowerLineMeasurement, AcPowerPortMeasurement, ActiveDiagnosticState,
    ClockMonotonic, DcMeasurementUpdate, DcPowerLineMeasurement, DcPowerPortMeasurement,
    DiagnosticTroubleCode, DtcSeverity, FailureModeIndicator, Identity, PowerElectronicsState,
    PowerHardwareUpdate, String32,
};

/// TMS port-number assignments (PROVISIONAL — see module docs). AC output is the inverter's primary
/// load-side output; the grid feed is the secondary utility input. PV and battery are the two DC
/// ports. Kept as named constants so the acceptance/e2e assertions and the doc rationale reference
/// one source of truth.
#[allow(dead_code)]
const PORT_AC_OUTPUT: i32 = 0;
#[allow(dead_code)]
const PORT_GRID_INPUT: i32 = 1;
#[allow(dead_code)]
const PORT_PV: i32 = 2;
#[allow(dead_code)]
const PORT_BATTERY: i32 = 3;

/// Scale helpers (raw fixed-point inverter units → SI engineering `f32`). Named by the canonical
/// field's raw-unit suffix, mirroring [`crate::transform::scale`]'s convention.
pub mod scale {
    /// Deci-volt (V×10) → volts.
    pub fn dv_to_v(dv: u16) -> f32 {
        dv as f32 / 10.0
    }
    /// Deci-hertz (Hz×10) → hertz.
    pub fn dhz_to_hz(dhz: u16) -> f32 {
        dhz as f32 / 10.0
    }
    /// Centi-volt (V×100) → volts.
    pub fn cv_to_v(cv: u16) -> f32 {
        cv as f32 / 100.0
    }
    /// Deci-amp (A×10) → amps.
    pub fn da_to_a(da: u16) -> f32 {
        da as f32 / 10.0
    }
}

/// Collect an iterator into a bounded `heapless::Vec`, dropping any excess past capacity. Local copy
/// of [`crate::transform`]'s private `bounded` (sibling modules can't reach each other's private
/// fns; this workspace's convention is each file carries its own tiny helper copy).
#[allow(dead_code)]
fn bounded<T, const N: usize>(iter: impl IntoIterator<Item = T>) -> Vec<T, N> {
    let mut v = Vec::new();
    for item in iter {
        if v.push(item).is_err() {
            break;
        }
    }
    v
}

// ---- AC measurement ---------------------------------------------------------------------------

/// The utility grid **input** AC line. QPIGS reports grid voltage + frequency only — no grid
/// current, no grid power — so amperage/real/reactive power are `NaN` (not-available), `phase_offset`
/// is `None`.
#[allow(dead_code)]
fn grid_line(r: &CanonicalInverterReading) -> AcPowerLineMeasurement {
    AcPowerLineMeasurement {
        voltage: scale::dv_to_v(r.grid_voltage_dv),
        frequency: scale::dhz_to_hz(r.grid_frequency_dhz),
        amperage: f32::NAN,
        phase_offset: None,
        real_power: f32::NAN,
        reactive_power: f32::NAN,
    }
}

/// The load-side AC **output** line. Amperage is derived from apparent power ÷ output voltage (the
/// only current QPIGS implies) when the voltage is non-zero, else `NaN`; `real_power` is the reported
/// active power; `reactive_power` has no QPIGS source and stays `NaN`.
#[allow(dead_code)]
fn ac_output_line(r: &CanonicalInverterReading) -> AcPowerLineMeasurement {
    let voltage = scale::dv_to_v(r.ac_output_voltage_dv);
    // I = S / V (apparent power VA ÷ RMS volts). Guard against a divide-by-zero when the output is
    // de-energized — an unknown current, not zero, so `NaN`.
    let amperage = if r.ac_output_voltage_dv > 0 {
        r.ac_output_apparent_power_va as f32 / voltage
    } else {
        f32::NAN
    };
    AcPowerLineMeasurement {
        voltage,
        frequency: scale::dhz_to_hz(r.ac_output_frequency_dhz),
        amperage,
        phase_offset: None,
        real_power: r.ac_output_active_power_w as f32,
        reactive_power: f32::NAN,
    }
}

#[allow(dead_code)]
fn ac_port(port_number: i32, line: AcPowerLineMeasurement) -> AcPowerPortMeasurement {
    AcPowerPortMeasurement {
        port_number,
        line: bounded([line]),
        phase_angle: Vec::new(),
        real_power_rate_of_change: f32::NAN,
        reactive_power_rate_of_change: f32::NAN,
    }
}

/// `tms::ac::MeasurementUpdate` — AC output (port 0) + grid input (port 1) on `external_measurement`.
/// GATED: test-only oracle (WP5/WP6 replaced this with [`crate::wire::AcMeasurementUpdateWire`]
/// render path). Kept for cross-oracle verification in `tests/wire_serde_cross_oracle.rs`.
#[cfg(feature = "std")]
pub fn to_ac_measurement(
    r: &CanonicalInverterReading,
    device_id: Identity,
    now: ClockMonotonic,
) -> AcMeasurementUpdate {
    AcMeasurementUpdate {
        device_id,
        time_measured: now,
        external_measurement: bounded([
            ac_port(PORT_AC_OUTPUT, ac_output_line(r)),
            ac_port(PORT_GRID_INPUT, grid_line(r)),
        ]),
        internal_measurement: Vec::new(),
    }
}

// ---- DC measurement ---------------------------------------------------------------------------

/// The PV/solar DC input line: `power = pv_charging_power` (reported directly, not V×A — the two are
/// separately reported and need not be exactly consistent on real hardware).
#[allow(dead_code)]
fn pv_line(r: &CanonicalInverterReading) -> DcPowerLineMeasurement {
    DcPowerLineMeasurement {
        voltage: scale::dv_to_v(r.pv_voltage_dv),
        amperage: scale::da_to_a(r.pv_current_da),
        power: r.pv_charging_power_w as f32,
    }
}

/// The battery DC-bus line. Amperage uses the source-positive TMS convention (`§2.7`): current is
/// positive when the battery *sources* into the converter (discharging), negative when it *sinks*
/// (charging) — so `amperage = discharge − charge`. `power = V × A` follows the same sign.
#[allow(dead_code)]
fn battery_line(r: &CanonicalInverterReading) -> DcPowerLineMeasurement {
    let voltage = scale::cv_to_v(r.battery_voltage_cv);
    let amperage = r.battery_discharge_current_a as f32 - r.battery_charging_current_a as f32;
    DcPowerLineMeasurement {
        voltage,
        amperage,
        power: voltage * amperage,
    }
}

#[allow(dead_code)]
fn dc_port(port_number: i32, line: DcPowerLineMeasurement) -> DcPowerPortMeasurement {
    DcPowerPortMeasurement {
        port_number,
        line: bounded([line]),
        power_rate_of_change: f32::NAN,
    }
}

/// `tms::dc::MeasurementUpdate` — PV (port 2) + battery DC (port 3) on `external_measurement`.
/// GATED: test-only oracle (WP5/WP6 replaced this with [`crate::wire::DcMeasurementUpdateInverterWire`]).
#[cfg(feature = "std")]
pub fn to_dc_measurement(
    r: &CanonicalInverterReading,
    device_id: Identity,
    now: ClockMonotonic,
) -> DcMeasurementUpdate {
    DcMeasurementUpdate {
        device_id,
        time_measured: now,
        external_measurement: bounded([
            dc_port(PORT_PV, pv_line(r)),
            dc_port(PORT_BATTERY, battery_line(r)),
        ]),
        internal_measurement: Vec::new(),
    }
}

// ---- Power hardware ---------------------------------------------------------------------------

/// `tms::PowerHardwareUpdate` — only `power_electronics.temperature` (the inverter heatsink temp).
/// A converter has no energy-storage/engine/fuel/generator/thermal subsystem this leg can source, so
/// those five `@optional` states stay `None`.
/// GATED: test-only oracle (WP5/WP6 replaced this with [`crate::wire::PowerHardwareUpdateInverterWire`]).
#[cfg(feature = "std")]
pub fn to_power_hardware(
    r: &CanonicalInverterReading,
    device_id: Identity,
    now: ClockMonotonic,
) -> PowerHardwareUpdate {
    PowerHardwareUpdate {
        device_id,
        timestamp: now,
        engine: None,
        fuel: None,
        generator: None,
        energy_storage: None,
        power_electronics: Some(PowerElectronicsState {
            temperature: r.inverter_temperature_c as f32,
        }),
        thermal: None,
    }
}

// ---- Diagnostics ------------------------------------------------------------------------------

/// One inverter fault-table entry: a QPIWS flag getter, its J1939 SPN, and a ≤32-char hint. Mirrors
/// [`crate::transform`]'s `FaultEntry` alias (factored out to keep the const's type simple).
#[allow(dead_code)]
type InverterFaultEntry = (fn(&CanonicalInverterFault) -> bool, u32, &'static str);

/// The inverter QPIWS fault flag ↔ (SPN, ≤32-char hint) table. `SPN = 100 + QPIWS bit index`
/// (line_fail=bit5→105, overtemperature=bit9→109, fan_locked=bit10→110, battery_low=bit12→112,
/// overload=bit15→115, output_short=bit17→117) — deliberately disjoint from the battery's SPN
/// 1..16 (see [`crate::transform`]'s `FAULT_TABLE`) so a unit running both legs never collides SPNs.
/// PROVISIONAL: SPN assignment is derived, not from a J1939-standard inverter DTC table.
#[allow(dead_code)]
const INVERTER_FAULT_TABLE: &[InverterFaultEntry] = &[
    (|f| f.line_fail, 105, "line fail"),
    (|f| f.overtemperature_fault, 109, "over temperature"),
    (|f| f.fan_locked_fault, 110, "fan locked"),
    (|f| f.battery_low_fault, 112, "battery low"),
    (|f| f.overload_fault, 115, "overload"),
    (|f| f.output_short_fault, 117, "output short"),
];

/// SPN for the synthesized QMOD-mode-fault DTC — the QMOD word's only TMS representation (D10; it
/// has no faithful topic of its own). Disjoint from both the inverter (105..117) and battery (1..16)
/// SPN ranges.
#[allow(dead_code)]
const SPN_MODE_FAULT: u32 = 130;

#[allow(dead_code)]
fn inverter_dtc(spn: u32, hint: &str, now: ClockMonotonic) -> DiagnosticTroubleCode {
    let mut h = String32::new();
    let _ = h.push_str(hint);
    DiagnosticTroubleCode {
        port_number: None,
        suspect_parameter: spn,
        failure_mode: FailureModeIndicator::ConditionExists,
        occurrence_count: 1,
        time_raised: now,
        severity: DtcSeverity::Major,
        hint: h,
        estimate_time: None,
        estimate_severity: None,
    }
}

/// `tms::ActiveDiagnosticState` — one DTC per set QPIWS bit, plus a synthesized SPN 130 DTC when
/// `mode == Fault`. `mode` is threaded in (unlike the battery `to_active_diagnostics`, which needs
/// no mode) precisely because the QMOD word has no faithful TMS home and this is its only
/// representation — folding it into the QPIWS-cadence diagnostic emit keeps `ActiveDiagnosticState`
/// a single-publisher topic (see D10 / `impl_app::host::HostPlatform::emit_inverter_fault`).
/// GATED: test-only oracle (WP5/WP6 replaced this with [`crate::wire::ActiveDiagnosticStateInverterWire`]).
#[cfg(feature = "std")]
pub fn to_active_diagnostics(
    fault: &CanonicalInverterFault,
    mode: InverterMode,
    device_id: Identity,
    now: ClockMonotonic,
) -> ActiveDiagnosticState {
    let mut codes = Vec::new();
    let mut overflow = 0u32;
    let mut push = |dtc: DiagnosticTroubleCode| {
        if codes.push(dtc).is_err() {
            overflow += 1;
        }
    };
    for &(is_set, spn, hint) in INVERTER_FAULT_TABLE {
        if is_set(fault) {
            push(inverter_dtc(spn, hint, now));
        }
    }
    if matches!(mode, InverterMode::Fault) {
        push(inverter_dtc(SPN_MODE_FAULT, "inverter fault mode", now));
    }
    ActiveDiagnosticState {
        device_id,
        codes,
        overflow,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use lib_parse::canonical::InverterRatings;

    fn dev() -> Identity {
        let mut s = Identity::new();
        s.push_str("METEOR-001-inv").unwrap();
        s
    }

    fn now() -> ClockMonotonic {
        ClockMonotonic {
            seconds: 42,
            nanoseconds: 7,
        }
    }

    /// A representative reading: 230.4 V / 50.0 Hz grid, 230.0 V / 50.0 Hz output, 900 VA / 850 W
    /// load, 51.20 V battery, 8 A charge / 0 A discharge, 27 °C, 5.0 A PV @ 380.0 V, 1900 W PV.
    fn sample() -> CanonicalInverterReading {
        CanonicalInverterReading {
            grid_voltage_dv: 2304,
            grid_frequency_dhz: 500,
            ac_output_voltage_dv: 2300,
            ac_output_frequency_dhz: 500,
            ac_output_apparent_power_va: 900,
            ac_output_active_power_w: 850,
            bus_voltage_v: 400,
            battery_voltage_cv: 5120,
            battery_charging_current_a: 8,
            battery_discharge_current_a: 0,
            inverter_temperature_c: 27,
            pv_current_da: 50,
            pv_voltage_dv: 3800,
            pv_charging_power_w: 1900,
            status_bits: 0,
            mode: InverterMode::Line,
            ratings: Some(InverterRatings {
                battery_type: heapless::String::new(),
                variant: heapless::String::new(),
                output_mode: heapless::String::new(),
            }),
        }
    }

    #[test]
    fn ac_measurement_has_output_then_grid_ports() {
        let ac = to_ac_measurement(&sample(), dev(), now());
        assert_eq!(ac.external_measurement.len(), 2);
        assert!(ac.internal_measurement.is_empty());
        let out = &ac.external_measurement[0];
        assert_eq!(out.port_number, PORT_AC_OUTPUT);
        assert!((out.line[0].voltage - 230.0).abs() < 1e-3);
        assert!((out.line[0].frequency - 50.0).abs() < 1e-3);
        // I = 900 VA / 230.0 V ≈ 3.913 A.
        assert!((out.line[0].amperage - (900.0 / 230.0)).abs() < 1e-3);
        assert!((out.line[0].real_power - 850.0).abs() < 1e-3);
        assert!(out.line[0].reactive_power.is_nan());
        let grid = &ac.external_measurement[1];
        assert_eq!(grid.port_number, PORT_GRID_INPUT);
        assert!((grid.line[0].voltage - 230.4).abs() < 1e-3);
        assert!(grid.line[0].amperage.is_nan());
        assert!(grid.line[0].real_power.is_nan());
        assert!(grid.line[0].phase_offset.is_none());
    }

    #[test]
    fn ac_output_amperage_is_nan_when_output_deenergized() {
        let mut r = sample();
        r.ac_output_voltage_dv = 0;
        let ac = to_ac_measurement(&r, dev(), now());
        assert!(ac.external_measurement[0].line[0].amperage.is_nan());
    }

    #[test]
    fn dc_measurement_has_pv_then_battery_ports_source_positive() {
        let dc = to_dc_measurement(&sample(), dev(), now());
        assert_eq!(dc.external_measurement.len(), 2);
        let pv = &dc.external_measurement[0];
        assert_eq!(pv.port_number, PORT_PV);
        assert!((pv.line[0].voltage - 380.0).abs() < 1e-3);
        assert!((pv.line[0].amperage - 5.0).abs() < 1e-3);
        assert!((pv.line[0].power - 1900.0).abs() < 1e-3);
        let batt = &dc.external_measurement[1];
        assert_eq!(batt.port_number, PORT_BATTERY);
        assert!((batt.line[0].voltage - 51.20).abs() < 1e-3);
        // charging 8 A, discharging 0 A -> source-positive amperage = 0 - 8 = -8 A.
        assert!((batt.line[0].amperage - (-8.0)).abs() < 1e-3);
        assert!((batt.line[0].power - (51.20 * -8.0)).abs() < 1e-2);
    }

    #[test]
    fn power_hardware_carries_only_power_electronics_temperature() {
        let hw = to_power_hardware(&sample(), dev(), now());
        assert!(hw.energy_storage.is_none());
        assert!(hw.engine.is_none());
        let pe = hw.power_electronics.expect("power_electronics present");
        assert!((pe.temperature - 27.0).abs() < 1e-3);
    }

    #[test]
    fn qpiws_bits_map_to_disjoint_spns() {
        let fault = CanonicalInverterFault {
            line_fail: true,
            overload_fault: true,
            output_short_fault: true,
            ..Default::default()
        };
        let diag = to_active_diagnostics(&fault, InverterMode::Line, dev(), now());
        let spns: heapless::Vec<u32, 8> = diag.codes.iter().map(|c| c.suspect_parameter).collect();
        assert!(spns.contains(&105));
        assert!(spns.contains(&115));
        assert!(spns.contains(&117));
        assert_eq!(diag.codes.len(), 3);
        // Disjoint from the battery SPN range (1..=16).
        assert!(diag.codes.iter().all(|c| c.suspect_parameter >= 100));
        assert!(diag
            .codes
            .iter()
            .all(|c| matches!(c.severity, DtcSeverity::Major)));
    }

    #[test]
    fn mode_fault_synthesizes_spn_130_even_with_no_qpiws_bits() {
        let diag = to_active_diagnostics(
            &CanonicalInverterFault::default(),
            InverterMode::Fault,
            dev(),
            now(),
        );
        assert_eq!(diag.codes.len(), 1);
        assert_eq!(diag.codes[0].suspect_parameter, SPN_MODE_FAULT);
    }

    #[test]
    fn no_faults_and_non_fault_mode_yields_empty_diagnostics() {
        let diag = to_active_diagnostics(
            &CanonicalInverterFault::default(),
            InverterMode::Line,
            dev(),
            now(),
        );
        assert!(diag.codes.is_empty());
        assert_eq!(diag.overflow, 0);
    }
}

//! Canonical → TMS conversions.
//!
//! Nested TMS structs are built by plain conversion functions and compose by calling one another
//! (not `From<&CanonicalReading>` — since the TMS types live in the standalone `lib-tms` crate,
//! neither the trait nor the target type is local here, so a `From` impl would violate the
//! orphan rule); the top-level topic payloads (which also need a device id + timestamp) are
//! produced by the [`ToTms`] / [`ToDiagnostics`] traits. Two conventions from the standard (§2.7)
//! are load-bearing:
//! * **Sign inversion** — the vendor reports positive current as *charging* (into the battery);
//!   TMS positive is *into the microgrid* (sourcing). So `tms_amperage = -vendor_current`.
//! * **Units** — centi-volts/amps → V/A (`/100`), millivolts → V (`/1000`), raw Kelvin×10 →
//!   °C (`(raw-2731)/10`), deci-percent → per-unit 0..1 (`/1000`).

use heapless::Vec;
use lib_parse::canonical::{CanonicalFault, CanonicalReading};
#[allow(unused_imports)]
use lib_tms::{
    ActiveDiagnosticState, Curve2D, DiagnosticTroubleCode, DtcSeverity, FailureModeIndicator,
    StorageUpdate, String32,
};
#[allow(unused_imports)]
use lib_tms::{
    ClockMonotonic, DcMeasurementUpdate, DcPowerLineMeasurement, DcPowerPortMeasurement,
    EnergyStorageState, Identity, PowerHardwareUpdate, StorageCellState,
};

/// Scale helpers (raw fixed-point → SI, and SI → raw fixed-point for the read/decode path).
///
/// The reverse (`*_to_*` → raw) helpers round to the nearest integer raw unit. For any value that
/// actually originated from the corresponding forward helper, this recovers the exact original raw
/// value: `f32` has a 24-bit mantissa, comfortably exact for the `u16`/`i16` ranges in play here, so
/// `raw as f32` → arithmetic → `round_f32(..) as raw` round-trips losslessly.
///
/// # Examples
///
/// ```
/// use lib_mapping::transform::scale;
///
/// assert_eq!(scale::cv_to_v(5451), 54.51); // centi-volts → volts
/// assert_eq!(scale::dk_to_c(2801), 7.0); // raw Kelvin×10 → °C
/// assert_eq!(scale::dpct_to_pu(775), 0.775); // deci-percent → per-unit SOC
///
/// // The reverse helpers recover the exact raw value for anything the forward helpers produced.
/// assert_eq!(scale::v_to_cv(scale::cv_to_v(5451)), 5451);
/// assert_eq!(scale::c_to_dk(scale::dk_to_c(2801)), 2801);
/// assert_eq!(scale::pu_to_dpct(scale::dpct_to_pu(775)), 775);
/// ```
pub mod scale {
    /// Round-half-away-from-zero, without `f32::round` (`core` has no float rounding — it's a
    /// `std`/`libm` method, unavailable on this crate's `no_std` MCU target). Truncating a
    /// pre-biased value toward zero (`as i32`, which Rust defines as saturating, never UB) gives
    /// the same result as `round()` for every magnitude in play here (raw telemetry scaled by
    /// ≤1000, always far inside `i32`'s range).
    fn round_f32(x: f32) -> f32 {
        let biased = if x >= 0.0 { x + 0.5 } else { x - 0.5 };
        (biased as i32) as f32
    }

    /// Centi-volts → volts.
    pub fn cv_to_v(cv: u16) -> f32 {
        cv as f32 / 100.0
    }
    /// Volts → centi-volts (inverse of [`cv_to_v`]).
    pub fn v_to_cv(v: f32) -> u16 {
        round_f32(v * 100.0) as u16
    }
    /// Signed centi-amps → amps.
    pub fn ca_to_a(ca: i16) -> f32 {
        ca as f32 / 100.0
    }
    /// Amps → signed centi-amps (inverse of [`ca_to_a`]).
    pub fn a_to_ca(a: f32) -> i16 {
        round_f32(a * 100.0) as i16
    }
    /// Millivolts → volts.
    pub fn mv_to_v(mv: u16) -> f32 {
        mv as f32 / 1000.0
    }
    /// Volts → millivolts (inverse of [`mv_to_v`]).
    pub fn v_to_mv(v: f32) -> u16 {
        round_f32(v * 1000.0) as u16
    }
    /// Raw Kelvin×10 → degrees Celsius.
    pub fn dk_to_c(raw: u16) -> f32 {
        (raw as f32 - 2731.0) / 10.0
    }
    /// Degrees Celsius → raw Kelvin×10 (inverse of [`dk_to_c`]).
    pub fn c_to_dk(c: f32) -> u16 {
        round_f32(c * 10.0 + 2731.0) as u16
    }
    /// Deci-percent (e.g. 775 = 77.5%) → per-unit 0..1.
    pub fn dpct_to_pu(dpct: u16) -> f32 {
        dpct as f32 / 1000.0
    }
    /// Per-unit 0..1 → deci-percent (inverse of [`dpct_to_pu`]).
    pub fn pu_to_dpct(pu: f32) -> u16 {
        round_f32(pu * 1000.0) as u16
    }
}

/// Collect an iterator into a bounded `heapless::Vec`, dropping any excess past capacity.
/// Used by both write-side (`ToTms`) and read-side (`FromTms`) conversions; stays ungated.
fn bounded<T, const N: usize>(iter: impl IntoIterator<Item = T>) -> Vec<T, N> {
    let mut v = Vec::new();
    for item in iter {
        if v.push(item).is_err() {
            break;
        }
    }
    v
}

/// (min, average, max) of the reading's temperatures in °C; NaN triple if there are none.
#[cfg(feature = "std")]
fn temp_stats(r: &CanonicalReading) -> (f32, f32, f32) {
    if r.temperatures_raw.is_empty() {
        return (f32::NAN, f32::NAN, f32::NAN);
    }
    let mut min = f32::INFINITY;
    let mut max = f32::NEG_INFINITY;
    let mut sum = 0.0;
    for &raw in r.temperatures_raw.iter() {
        let c = scale::dk_to_c(raw);
        min = min.min(c);
        max = max.max(c);
        sum += c;
    }
    (min, sum / r.temperatures_raw.len() as f32, max)
}

/// Temperature of the sensor nearest cell `i` (clamped to the last sensor); NaN if none.
#[cfg(feature = "std")]
fn nearest_temp(r: &CanonicalReading, i: usize) -> f32 {
    match r.temperatures_raw.len() {
        0 => f32::NAN,
        n => scale::dk_to_c(r.temperatures_raw[core::cmp::min(i, n - 1)]),
    }
}

// ---- nested conversions (no external context needed) ----

#[cfg(feature = "std")]
fn power_line_measurement_from(r: &CanonicalReading) -> DcPowerLineMeasurement {
    let voltage = scale::cv_to_v(r.total_voltage_cv);
    let amperage = -scale::ca_to_a(r.current_ca); // SIGN INVERSION (vendor charge+ → TMS source+)
    DcPowerLineMeasurement {
        voltage,
        amperage,
        power: voltage * amperage,
    }
}

#[cfg(feature = "std")]
fn power_port_measurement_from(r: &CanonicalReading) -> DcPowerPortMeasurement {
    DcPowerPortMeasurement {
        port_number: 0,
        line: bounded([power_line_measurement_from(r)]),
        power_rate_of_change: f32::NAN,
    }
}

/// `#[inline(never)]` — this builds the 50-slot `StorageCellState` `Vec` that dominates
/// `render_tms`'s transient frame for `power_hardware_update`; keeping it a real call (not inlined
/// into `to_power_hardware`/`render_tms`) helps the compiler keep the transient frame local to the
/// call rather than merging it further upward. See `render_tms`'s doc comment for the broker-thread
/// stack-overflow context this is part of preventing. No logic/output change.
///
/// **GATED: test-only oracle (WP8).** Production renders via [`crate::render::render_tms`].
#[inline(never)]
#[cfg(feature = "std")]
fn energy_storage_state_from(r: &CanonicalReading) -> EnergyStorageState {
    let (min_temperature, average_temperature, max_temperature) = temp_stats(r);
    let cells = r
        .cell_voltages_mv
        .iter()
        .enumerate()
        .map(|(i, &mv)| StorageCellState {
            cell_number: i as u16,
            cycle_counter: r.cycles as f32,
            // #278: the IDL marks `stateOfCharge` `@optional`, so the generated `StorageCellState`
            // carries `Option<f32>`. `Some(x)` serializes byte-identically to a bare `f32` — wire
            // bytes are unchanged (the golden hand-written struct had it as bare `f32`).
            state_of_charge: Some(scale::dpct_to_pu(r.soc_dpct)),
            temperature: nearest_temp(r, i),
            voltage: scale::mv_to_v(mv),
        });
    EnergyStorageState {
        min_temperature,
        average_temperature,
        max_temperature,
        run_time: f32::NAN,
        storage_cells: bounded(cells),
    }
}

// ---- top-level topic payloads (need device id + timestamp) ----

/// Convert a canonical battery reading into TMS topic payloads.
///
/// **GATED: test-only oracle (WP8).** Production code uses the wire-serializer
/// ([`crate::render::render_tms`]); this trait survives only as a cross-oracle verification path.
///
/// # Examples
///
/// One Seplos telemetry frame maps to the `DcMeasurementUpdate`/`PowerHardwareUpdate` topic pair,
/// with the §2.7 conventions applied — sign inversion on the pack current, raw fixed-point → SI:
///
/// ```
/// use lib_mapping::schema::{CanonicalReading, ClockMonotonic, Identity};
/// use lib_mapping::transform::ToTms;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// // 54.51 V pack, discharging 1.44 A (vendor charge-positive), 3 cells, 77.5 % SOC.
/// let reading = CanonicalReading {
///     total_voltage_cv: 5451,
///     current_ca: -144,
///     soc_dpct: 775,
///     cell_voltages_mv: heapless::Vec::from_slice(&[3894, 3893, 3895])?,
///     temperatures_raw: heapless::Vec::from_slice(&[2801, 2811])?, // 7.0 °C, 8.0 °C
/// #     device_address: 0,
/// #     remaining_capacity_cah: 13953,
/// #     typical_capacity_cah: 18000,
/// #     rated_capacity_cah: 18000,
/// #     soh_dpct: 1000,
/// #     cycles: 2,
/// #     port_voltage_cv: 5453,
///     // …remaining raw fields hidden…
/// };
/// let device_id = Identity::try_from("METEOR-001")?;
/// let now = ClockMonotonic { seconds: 1_000, nanoseconds: 0 };
///
/// let dc = reading.to_dc_measurement(device_id.clone(), now);
/// let line = &dc.external_measurement[0].line[0];
/// assert_eq!(line.voltage, 54.51); // centi-volts → volts
/// assert_eq!(line.amperage, 1.44); // sign inversion: vendor discharge − → TMS sourcing +
///
/// let hw = reading.to_power_hardware(device_id, now);
/// let storage = hw.energy_storage.as_ref().ok_or("battery reading always fills storage")?;
/// assert_eq!(storage.storage_cells.len(), 3);
/// assert_eq!(storage.storage_cells[0].voltage, 3.894); // millivolts → volts
/// assert_eq!(storage.storage_cells[0].state_of_charge, Some(0.775)); // deci-percent → per-unit
/// assert_eq!(storage.storage_cells[0].temperature, 7.0); // nearest sensor, Kelvin×10 → °C
/// # Ok(()) }
/// ```
#[cfg(feature = "std")]
pub trait ToTms {
    /// `DcMeasurementUpdate` — port 0, one external DC line (sign-inverted amperage).
    fn to_dc_measurement(&self, device_id: Identity, now: ClockMonotonic) -> DcMeasurementUpdate;
    /// `PowerHardwareUpdate` carrying `EnergyStorageState`.
    fn to_power_hardware(&self, device_id: Identity, now: ClockMonotonic) -> PowerHardwareUpdate;
}

#[cfg(feature = "std")]
impl ToTms for CanonicalReading {
    fn to_dc_measurement(&self, device_id: Identity, now: ClockMonotonic) -> DcMeasurementUpdate {
        DcMeasurementUpdate {
            device_id,
            time_measured: now,
            external_measurement: bounded([power_port_measurement_from(self)]),
            internal_measurement: Vec::new(),
        }
    }

    fn to_power_hardware(&self, device_id: Identity, now: ClockMonotonic) -> PowerHardwareUpdate {
        PowerHardwareUpdate {
            device_id,
            timestamp: now,
            // A battery-only device populates only `energy_storage`; the other five `@optional`
            // subsystem states have no Seplos source and stay `None`.
            engine: None,
            fuel: None,
            generator: None,
            energy_storage: Some(energy_storage_state_from(self)),
            power_electronics: None,
            thermal: None,
        }
    }
}

/// Convert a canonical battery reading into a `StorageUpdate` topic payload — the STOR-role topic
/// whose *constant* already existed with no payload type (see
/// `docs/gap-analysis-meteor-parity.md`'s "StorageUpdate payload type" finding).
///
/// Wired end to end (#240): `impl_app::host::HostPlatform::emit_reading` calls this every
/// successful poll cycle and publishes the result to `topics::STORAGE_UPDATE` alongside
/// `DcMeasurementUpdate`/`PowerHardwareUpdate` (see `multi_port_e2e.rs`'s
/// `two_buses_share_one_broker_and_interleave_publishes` for a live-broker assertion this
/// actually reaches a subscriber). Maps to the MIL-STD-3071 2.0.2 `StorageUpdate` shape
/// (`internal_voltage`/`state_of_charge`/`available_energy`/`hold_time`/`*_time` curves — reshaped
/// from the earlier `EnergyStorageState`-based form during the IDL parity pass) — PROVISIONAL fill:
/// the baseline never populated `StorageUpdate`, so this canonical→StorageUpdate mapping is novel
/// (see `docs/compliance/tms/mil-std-3071_data_model.idl` and `lib_tms::StorageUpdate`'s doc).
/// The `tms_breadth` acceptance feature (`test-acceptance/tests/features/tms_breadth.feature`)
/// asserts the mapped payload stays within [`crate::sink::MAX_PAYLOAD`] even for a
/// maximally-populated reading.
/// GATED: test-only oracle (WP5/WP6 replaced this with [`crate::wire::StorageUpdateWire`]
/// render path). Kept for cross-oracle verification.
#[cfg(feature = "std")]
pub trait ToStorageUpdate {
    fn to_storage_update(&self, device_id: Identity, now: ClockMonotonic) -> StorageUpdate;
}

#[cfg(feature = "std")]
impl ToStorageUpdate for CanonicalReading {
    fn to_storage_update(&self, device_id: Identity, _now: ClockMonotonic) -> StorageUpdate {
        // Ah = remaining_capacity_cah / 100; V = total_voltage_cv / 100; J = Ah × V × 3600.
        let available_energy = (self.remaining_capacity_cah as f32 / 100.0)
            * (self.total_voltage_cv as f32 / 100.0)
            * 3600.0;
        StorageUpdate {
            device_id,
            internal_voltage: scale::cv_to_v(self.total_voltage_cv),
            state_of_charge: Some(scale::dpct_to_pu(self.soc_dpct)),
            available_energy,
            // No Seplos source for hold-time; curves are empty; rate limits are unknown.
            hold_time: f32::NAN,
            charge_time: Curve2D { points: Vec::new() },
            discharge_time: Curve2D { points: Vec::new() },
            max_charge_time: Curve2D { points: Vec::new() },
            max_discharge_time: Curve2D { points: Vec::new() },
            max_charge_rate: None,
            max_discharge_rate: None,
        }
    }
}

/// One fault-table entry: a flag getter, a flag setter (used by the read/decode direction), its
/// J1939 SPN, and a ≤32-char hint. Getter + setter share this single table (rather than living in
/// separate write/read tables) so the SPN↔flag correspondence has exactly one source of truth and
/// the two directions cannot drift apart.
type FaultEntry = (
    fn(&CanonicalFault) -> bool,
    fn(&mut CanonicalFault),
    u32,
    &'static str,
);

/// The fault flag ↔ (J1939 SPN, ≤32-char hint) table. Each set flag emits one DTC on write; each
/// recognised SPN sets its flag back on read.
const FAULT_TABLE: &[FaultEntry] = &[
    (
        |f| f.high_voltage_protection,
        |f| f.high_voltage_protection = true,
        1,
        "cell overvoltage",
    ),
    (
        |f| f.low_voltage_protection,
        |f| f.low_voltage_protection = true,
        2,
        "cell undervoltage",
    ),
    (
        |f| f.high_temperature_charge_protection,
        |f| f.high_temperature_charge_protection = true,
        3,
        "hi-temp charge",
    ),
    (
        |f| f.low_temperature_charge_protection,
        |f| f.low_temperature_charge_protection = true,
        4,
        "lo-temp charge",
    ),
    (
        |f| f.charge_overcurrent_protection,
        |f| f.charge_overcurrent_protection = true,
        5,
        "charge overcurrent",
    ),
    (
        |f| f.discharge_overcurrent_protection,
        |f| f.discharge_overcurrent_protection = true,
        6,
        "discharge overcurrent",
    ),
    (
        |f| f.short_circuit_protection,
        |f| f.short_circuit_protection = true,
        7,
        "short circuit",
    ),
    (
        |f| f.mosfet_fault,
        |f| f.mosfet_fault = true,
        8,
        "mosfet fault",
    ),
    (
        |f| f.cell_balance_warning,
        |f| f.cell_balance_warning = true,
        9,
        "cell imbalance",
    ),
    (
        |f| f.low_capacity_warning,
        |f| f.low_capacity_warning = true,
        10,
        "low capacity",
    ),
    (
        |f| f.high_temperature_discharge_warning,
        |f| f.high_temperature_discharge_warning = true,
        11,
        "hi-temp discharge",
    ),
    (
        |f| f.low_temperature_discharge_warning,
        |f| f.low_temperature_discharge_warning = true,
        12,
        "lo-temp discharge",
    ),
    (
        |f| f.low_voltage_warning,
        |f| f.low_voltage_warning = true,
        13,
        "low voltage warn",
    ),
    (
        |f| f.high_voltage_warning,
        |f| f.high_voltage_warning = true,
        14,
        "high voltage warn",
    ),
    (
        |f| f.discharge_current_warning,
        |f| f.discharge_current_warning = true,
        15,
        "discharge current",
    ),
    (
        |f| f.charge_current_warning,
        |f| f.charge_current_warning = true,
        16,
        "charge current",
    ),
];

/// Convert canonical fault flags into a TMS diagnostics payload.
///
/// **GATED: test-only oracle (WP8).** Production code uses the wire-serializer
/// ([`crate::render::render_fault`]); this trait survives only as a cross-oracle verification path.
#[cfg(feature = "std")]
pub trait ToDiagnostics {
    /// `ActiveDiagnosticState` — one `ConditionExists`/`Major` DTC per set flag, ≤64 + overflow.
    fn to_active_diagnostics(
        &self,
        device_id: Identity,
        now: ClockMonotonic,
    ) -> ActiveDiagnosticState;
}

#[cfg(feature = "std")]
impl ToDiagnostics for CanonicalFault {
    /// `#[inline(never)]` — this builds the DTC-list `Vec` that dominates `render_fault`'s
    /// transient frame (measured 7,200 B pre-split). See `render_fault`'s doc comment for the
    /// broker-thread stack-overflow context. No logic/output change.
    #[inline(never)]
    fn to_active_diagnostics(
        &self,
        device_id: Identity,
        now: ClockMonotonic,
    ) -> ActiveDiagnosticState {
        let mut codes = Vec::new();
        let mut overflow = 0u32;
        for &(is_set, _setter, spn, hint) in FAULT_TABLE {
            if is_set(self) {
                let mut h = String32::new();
                let _ = h.push_str(hint);
                let dtc = DiagnosticTroubleCode {
                    port_number: None,
                    suspect_parameter: spn,
                    failure_mode: FailureModeIndicator::ConditionExists,
                    occurrence_count: 1,
                    time_raised: now,
                    severity: DtcSeverity::Major,
                    hint: h,
                    // Seplos carries no prognostic model — these two @optional IDL fields are
                    // always absent on write.
                    estimate_time: None,
                    estimate_severity: None,
                };
                if codes.push(dtc).is_err() {
                    overflow += 1;
                }
            }
        }
        ActiveDiagnosticState {
            device_id,
            codes,
            overflow,
        }
    }
}

// ---- read/decode direction: TMS -> canonical ----
//
// These invert `ToTms`/`ToDiagnostics`. Every conversion documents exactly which fields round-trip
// exactly and which are lossy or entirely unrepresentable in TMS, per the standard's own field set
// (never invented values — an absent TMS field decodes to an explicit, documented placeholder, not
// a guess).

/// Best-effort inverse of the write-side `nearest_temp` per-cell sensor assignment (see
/// `energy_storage_state_from`): a raw sensor value repeats across every cell
/// from its first assignment onward, so collapsing consecutive equal per-cell temperatures (in
/// cell order) recovers the original sensor sequence *exactly* whenever
/// `cells.len() >= original_sensor_count`. When there were originally MORE sensors than cells, the
/// sensors beyond `cells.len()` were never assigned to any cell and are permanently unrecoverable
/// — an information loss inherent to the standard's per-cell-not-per-sensor model, not a mapper
/// defect. `NaN` cell temperatures (no sensor data at all) decode to an empty sequence.
fn reconstruct_temps_raw(
    cells: &[StorageCellState],
) -> Vec<u16, { lib_parse::canonical::MAX_TEMPS }> {
    let mut out = Vec::new();
    let mut last: Option<f32> = None;
    for cell in cells {
        if cell.temperature.is_nan() {
            continue;
        }
        if last != Some(cell.temperature) {
            if out.push(scale::c_to_dk(cell.temperature)).is_err() {
                break;
            }
            last = Some(cell.temperature);
        }
    }
    out
}

/// Decode TMS topic payloads back into a canonical battery reading.
///
/// # Examples
///
/// The electrical fields survive a full write → read cycle exactly (see
/// [`FromTms::from_tms`] for the precise round-trip/lossy contract):
///
/// ```
/// use lib_mapping::schema::{CanonicalReading, ClockMonotonic, Identity};
/// use lib_mapping::transform::{FromTms, ToTms};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let reading = CanonicalReading {
///     total_voltage_cv: 5451,
///     current_ca: -144, // discharging 1.44 A, vendor charge-positive
///     cell_voltages_mv: heapless::Vec::from_slice(&[3894, 3893, 3895])?,
///     temperatures_raw: heapless::Vec::from_slice(&[2801, 2811])?,
/// #     soc_dpct: 775,
/// #     device_address: 0,
/// #     remaining_capacity_cah: 13953,
/// #     typical_capacity_cah: 18000,
/// #     rated_capacity_cah: 18000,
/// #     soh_dpct: 1000,
/// #     cycles: 2,
/// #     port_voltage_cv: 5453,
///     // …remaining raw fields hidden…
/// };
/// let device_id = Identity::try_from("METEOR-001")?;
/// let now = ClockMonotonic { seconds: 1_000, nanoseconds: 0 };
///
/// // Write side: the topic pair a single telemetry frame produces…
/// let dc = reading.to_dc_measurement(device_id.clone(), now);
/// let hw = reading.to_power_hardware(device_id, now);
///
/// // …read side: decode them back.
/// let decoded = CanonicalReading::from_tms(&dc, &hw);
/// assert_eq!(decoded.total_voltage_cv, reading.total_voltage_cv);
/// assert_eq!(decoded.current_ca, reading.current_ca); // sign re-inverted on the way back
/// assert_eq!(decoded.cell_voltages_mv, reading.cell_voltages_mv);
/// assert_eq!(decoded.temperatures_raw, reading.temperatures_raw); // cells ≥ sensors: exact
/// assert_eq!(decoded.soc_dpct, reading.soc_dpct);
/// // No TMS field carries pack capacity — it decodes to the documented `0` placeholder.
/// assert_eq!(decoded.remaining_capacity_cah, 0);
/// # Ok(()) }
/// ```
pub trait FromTms {
    /// Best-effort decode of a `CanonicalReading` from the `DcMeasurementUpdate` +
    /// `PowerHardwareUpdate` pair a single Seplos telemetry frame produces (see
    /// `SeplosBmsToTMSTransformer::_transform_telemetry` in the meteor prototype for the same
    /// one-frame-to-two-topics shape).
    ///
    /// Round-trips exactly (for values in normal battery ranges): `total_voltage_cv`,
    /// `current_ca` (sign re-inverted back to the vendor charge-positive convention),
    /// `cell_voltages_mv` (one entry per `storage_cells` element), `soc_dpct` and `cycles` (read
    /// from the first storage cell — Seplos reports one pack-level SOC/cycle-count broadcast
    /// identically to every cell on write, so any cell recovers the same value).
    ///
    /// LOSSY / NOT RECOVERABLE (documented, not silently guessed):
    /// * `temperatures_raw` — see `reconstruct_temps_raw`; exact only when
    ///   `storage_cells.len() >= the original sensor count`.
    /// * `remaining_capacity_cah`, `typical_capacity_cah`, `rated_capacity_cah`, `soh_dpct`,
    ///   `port_voltage_cv` — MIL-STD-3071's `dc::MeasurementUpdate` / `EnergyStorageState` carry no
    ///   field for pack capacity, state of health, or auxiliary port voltage at all, so there is
    ///   nothing in a TMS message to decode. These are set to `0`, which is a placeholder, **not**
    ///   a measurement of zero — callers needing these values must source them out-of-band.
    /// * `device_address` — TMS identifies devices by the `Identity` string, not the Seplos DIP
    ///   address; set to `0`.
    fn from_tms(dc: &DcMeasurementUpdate, hw: &PowerHardwareUpdate) -> CanonicalReading;
}

impl FromTms for CanonicalReading {
    fn from_tms(dc: &DcMeasurementUpdate, hw: &PowerHardwareUpdate) -> CanonicalReading {
        let (total_voltage_cv, current_ca) = dc
            .external_measurement
            .first()
            .and_then(|port| port.line.first())
            .map(|line| {
                let cv = scale::v_to_cv(line.voltage);
                // Undo the write-side sign inversion: TMS source-positive -> vendor charge-positive.
                let ca = scale::a_to_ca(-line.amperage);
                (cv, ca)
            })
            .unwrap_or((0, 0));

        let mut cell_voltages_mv: Vec<u16, { lib_parse::canonical::MAX_CELLS }> = Vec::new();
        let mut soc_dpct = 0u16;
        let mut cycles = 0u16;
        let mut temperatures_raw: Vec<u16, { lib_parse::canonical::MAX_TEMPS }> = Vec::new();

        if let Some(es) = &hw.energy_storage {
            cell_voltages_mv = bounded(es.storage_cells.iter().map(|c| scale::v_to_mv(c.voltage)));
            if let Some(first) = es.storage_cells.first() {
                // #278: `state_of_charge` is now `Option<f32>` (IDL `@optional`); an absent SOC
                // decodes to `0` deci-percent (a documented placeholder, not a measurement).
                soc_dpct = first.state_of_charge.map(scale::pu_to_dpct).unwrap_or(0);
                cycles = first.cycle_counter as u16;
            }
            temperatures_raw = reconstruct_temps_raw(&es.storage_cells);
        }

        CanonicalReading {
            device_address: 0,
            cell_voltages_mv,
            temperatures_raw,
            current_ca,
            total_voltage_cv,
            remaining_capacity_cah: 0,
            typical_capacity_cah: 0,
            rated_capacity_cah: 0,
            soc_dpct,
            soh_dpct: 0,
            cycles,
            port_voltage_cv: 0,
        }
    }
}

/// Decode a TMS diagnostics payload back into canonical fault flags.
pub trait FromDiagnostics {
    /// Best-effort decode of `CanonicalFault` boolean flags from `ActiveDiagnosticState.codes`.
    ///
    /// Round-trips exactly for every DTC whose `suspect_parameter` matches a Seplos SPN emitted by
    /// [`ToDiagnostics::to_active_diagnostics`] — the flag set is fully recovered, since a set flag
    /// always emits exactly one DTC on write and this reverses through the same `FAULT_TABLE`.
    ///
    /// LOSSY: a DTC from a foreign (non-Seplos) publisher whose `suspect_parameter` is not in the
    /// Seplos fault table has no corresponding canonical flag and is silently dropped — there is no
    /// Seplos-side representation for an unrecognised SPN, not a decode bug. `occurrence_count`,
    /// `time_raised`, `severity`, and `hint` are not represented in `CanonicalFault` at all (it is
    /// boolean-flags-only) and are discarded.
    fn from_active_diagnostics(state: &ActiveDiagnosticState) -> CanonicalFault;
}

impl FromDiagnostics for CanonicalFault {
    fn from_active_diagnostics(state: &ActiveDiagnosticState) -> CanonicalFault {
        let mut fault = CanonicalFault {
            device_address: 0,
            ..Default::default()
        };
        for code in state.codes.iter() {
            for &(_getter, setter, spn, _hint) in FAULT_TABLE {
                if code.suspect_parameter == spn {
                    setter(&mut fault);
                    break;
                }
            }
        }
        fault
    }
}

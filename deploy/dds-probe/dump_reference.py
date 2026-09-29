#!/usr/bin/env python3
"""dump_reference.py — the CycloneDDS-authoritative XCDR2 byte oracle (story #323 / WP5).

Two jobs:

1. `arbitration` — the empirical answer to the WP5 open question: how does CycloneDDS encode an
   `@optional` member of an APPENDABLE type under XCDR2, present AND absent? Reproduces the smallest
   optional-bearing TMS type (`ActiveMicrogridControllerState`) with CycloneDDS's OWN encoder, dumps
   both cases, and — the strongest oracle available — feeds our OWN COMMITTED hand-derived byte
   vectors (`crates/lib-mapping/tests/wire_cdr_cross_oracle.rs`) into CycloneDDS's OWN decoder to
   prove ACCEPTANCE, not just visual byte agreement.

2. `fixtures` — for every golden-fixture topic (`crates/lib-mapping/tests/fixtures/xcdr2/*.hex`,
   `wire_cdr_golden_bytes.rs::all_fixtures`), builds the same canned values through a CycloneDDS
   `IdlStruct` mirror (`tms_mirror_types.py`) and hex-dumps the CycloneDDS encoding, optionally
   diffing against the committed goldens (`--compare <fixtures-dir>`).

KNOWN, DOCUMENTED CAVEAT (both modes): `cyclonedds.idl.IdlStruct.serialize()` does NOT apply the
RTPS-level trailing encapsulation padding (DDS-XTypes 1.3 §7.6.3.1.2 — pad the payload to the next
4-byte boundary, record the pad count in the encapsulation options' low 2 bits). Verified empirically
against a MANDATORY (non-optional) single-string type at multiple lengths — the omission is
unconditional, not specific to `@optional`. `finalize_trailing_pad()` below applies it exactly like
`lib_tms::cdr::CdrWriteSink::finalize_encapsulation_padding`, so dumps reflect the true RTPS
SerializedPayload, not the convenience API's raw form.

SEPARATE, OUT-OF-SCOPE FINDING (surfaced, NOT fixed here — see the WP5 report): CycloneDDS's own
encoder ALSO prefixes every sequence-of-NON-PRIMITIVE-element field (struct or string elements) with
an extra 4-byte NEXTINT length delimiter that our `lib_tms::cdr::emit_appendable_seq`/
`emit_string_seq` do NOT emit. This is DDS-XTypes 1.3 rule (12) — CycloneDDS implements it; the
current `dust_dds` 0.15.0 byte-oracle does not (a live, open interop dispute — see
eclipse-cyclonedds/cyclonedds-cxx#218 and OMG issue DDSXTY14-56). It is UNRELATED to the `@optional`
arbitration (this file's job 1) and out of scope for WP5 (`emit_optional`/`emit_optional_seq` is the
single authorized flip point) — `fixtures` mode surfaces it per-topic as `SEQ_HEADER_DIVERGENCE`
rather than silently failing or hiding it.
"""
from __future__ import annotations

import argparse
import json
import math
import struct
import sys
from pathlib import Path

import tms_mirror_types as m

FIXTURE_DIR_DEFAULT = "crates/lib-mapping/tests/fixtures/xcdr2"


# ==================================================================================================
# byte helpers
# ==================================================================================================

def hexdump(b: bytes) -> str:
    return " ".join(f"{x:02x}" for x in b)


def from_hex(s: str) -> bytes:
    return bytes(int(t, 16) for t in s.split())


def finalize_trailing_pad(raw: bytes) -> bytes:
    """Apply the RTPS encapsulation trailing-pad rule CycloneDDS's `IdlStruct.serialize()` omits
    (see module doc): pad `raw` so the total length is a multiple of 4, and patch the encapsulation
    options' low byte's low 2 bits with the pad count (DDS-XTypes 1.3 §7.6.3.1.2). `raw[0:2]` is the
    encapsulation id (`D_CDR2_LE`), `raw[2:4]` are the options bytes (CycloneDDS always emits zeroed
    options — verified empirically — so this only ever ADDS the pad count, never overwrites a real
    upstream option bit).
    """
    assert len(raw) >= 4, "too short to be an encapsulated payload"
    pad = (-len(raw)) % 4
    out = bytearray(raw)
    out[3] = pad  # options low byte, low 2 bits = trailing pad count
    out.extend([0] * pad)
    return bytes(out)


def f32(x: float) -> float:
    """Round `x` to the nearest IEEE-754 binary32 (f32) value, exactly like Rust's `as f32` /
    `f32` arithmetic — used at every intermediate step in the fixture-value reconstructions below so
    Python's f64 arithmetic doesn't accumulate a different rounding than the real Rust f32 chain
    (`lib_mapping::wire::*_cdr` views compute entirely in f32)."""
    return struct.unpack("<f", struct.pack("<f", x))[0]


def round_div_i64(numerator: int, denominator: int) -> int:
    """Port of `lib_mapping::wire::generated_cdr_encoders::round_div_i64` — round-half-away-from-zero
    integer division, `denominator > 0`."""
    if numerator >= 0:
        return (numerator + denominator // 2) // denominator
    return -((-numerator + denominator // 2) // denominator)


NAN = f32(math.nan)


# ==================================================================================================
# fixture-value reconstructions (mirrors crates/lib-mapping/tests/wire_cdr_golden_bytes.rs +
# crates/lib-mapping/src/wire/{generated_cdr_encoders.rs,storage_cdr,inverter_cdr}/*)
# ==================================================================================================

FAULT_TABLE = [
    (1, 1, "cell overvoltage"),
    (2, 2, "cell undervoltage"),
    (4, 3, "hi-temp charge"),
    (8, 4, "lo-temp charge"),
    (16, 5, "charge overcurrent"),
    (32, 6, "discharge overcurrent"),
    (64, 7, "short circuit"),
    (128, 8, "mosfet fault"),
    (256, 9, "cell imbalance"),
    (512, 10, "low capacity"),
    (1024, 11, "hi-temp discharge"),
    (2048, 12, "lo-temp discharge"),
    (4096, 13, "low voltage warn"),
    (8192, 14, "high voltage warn"),
    (16384, 15, "discharge current"),
    (32768, 16, "charge current"),
]

INVERTER_FAULT_TABLE = [
    ("line_fail", 105, "line fail"),
    ("overtemperature_fault", 109, "over temperature"),
    ("fan_locked_fault", 110, "fan locked"),
    ("battery_low_fault", 112, "battery low"),
    ("overload_fault", 115, "overload"),
    ("output_short_fault", 117, "output short"),
]


def clock(sec: int, nsec: int) -> m.ClockMonotonic:
    return m.ClockMonotonic(seconds=sec, nanoseconds=nsec)


class Reading:
    """Mirrors `CanonicalReading` for `reading(14, 6, -164, 5452, 775, 2)`."""

    def __init__(self, cells: int, temps: int, current_ca: int, voltage_cv: int, soc: int, cycles: int):
        self.cell_voltages_mv = [3800 + i for i in range(cells)]
        self.temperatures_raw = [2731 + 250 + i * 2 for i in range(temps)]
        self.current_ca = current_ca
        self.total_voltage_cv = voltage_cv
        self.remaining_capacity_cah = 1395
        self.soc_dpct = soc
        self.cycles = cycles


def dc_measurement(r: Reading, device_id: str, t: m.ClockMonotonic) -> m.DcMeasurementUpdate:
    voltage = f32(r.total_voltage_cv / 100.0)
    amperage = f32(-f32(r.current_ca) / 100.0)
    power = f32(voltage * amperage)
    line = m.DcPowerLineMeasurement(voltage=voltage, amperage=amperage, power=power)
    port = m.DcPowerPortMeasurement(port_number=0, line=[line], power_rate_of_change=NAN)
    return m.DcMeasurementUpdate(
        device_id=device_id, time_measured=t, external_measurement=[port], internal_measurement=[]
    )


def energy_storage_state(r: Reading) -> m.EnergyStorageState:
    temps = r.temperatures_raw
    if not temps:
        min_t = avg_t = max_t = NAN
    else:
        min_raw, max_raw = min(temps), max(temps)
        min_t = f32((min_raw - 2731.0) / 10.0)
        max_t = f32((max_raw - 2731.0) / 10.0)
        n = len(temps)
        numerator_tenths_x1000 = (sum(temps) - 2731 * n) * 1000
        avg_milli = round_div_i64(numerator_tenths_x1000, n * 10)
        avg_t = f32(avg_milli / 1000.0)
    cells = []
    n_temps = len(temps)
    for i, mv in enumerate(r.cell_voltages_mv):
        if n_temps == 0:
            cell_temp = NAN
        else:
            raw = temps[min(i, n_temps - 1)]
            cell_temp = f32((raw - 2731.0) / 10.0)
        cells.append(
            m.StorageCellState(
                cell_number=i,
                cycle_counter=f32(r.cycles),
                state_of_charge=f32(r.soc_dpct / 1000.0),
                temperature=cell_temp,
                voltage=f32(mv / 1000.0),
            )
        )
    return m.EnergyStorageState(
        min_temperature=min_t, average_temperature=avg_t, max_temperature=max_t, run_time=NAN,
        storage_cells=cells,
    )


def power_hardware(r: Reading, device_id: str, t: m.ClockMonotonic) -> m.PowerHardwareUpdate:
    return m.PowerHardwareUpdate(
        device_id=device_id, timestamp=t, engine=None, fuel=None, generator=None,
        energy_storage=energy_storage_state(r), power_electronics=None, thermal=None,
    )


def diagnostic_codes(bits: int, t: m.ClockMonotonic) -> list:
    """Battery `ActiveDiagnosticState.codes` from a fault bitmask (mirrors `FAULT_TABLE` iteration in
    `generated_cdr_encoders.rs`). The inverter variant has its own table/entrypoint
    (`active_diagnostic_inverter`, below) — the two never share bit-index semantics."""
    codes = []
    for bit, spn, hint in FAULT_TABLE:
        if not bits & bit:
            continue
        codes.append(
            m.DiagnosticTroubleCode(
                port_number=None, suspect_parameter=spn,
                failure_mode=m.FailureModeIndicator.CONDITION_EXISTS, occurrence_count=1,
                time_raised=t, severity=m.DtcSeverity.MAJOR, estimate_time=None,
                estimate_severity=None, hint=hint,
            )
        )
    return codes


def active_diagnostic(bits: int, device_id: str, t: m.ClockMonotonic) -> m.ActiveDiagnosticState:
    return m.ActiveDiagnosticState(device_id=device_id, codes=diagnostic_codes(bits, t), overflow=0)


def storage_update(r: Reading, device_id: str) -> m.StorageUpdate:
    internal_voltage = f32(r.total_voltage_cv / 100.0)
    available_energy = f32(f32(r.remaining_capacity_cah / 100.0) * internal_voltage * 3600.0)
    empty_curve = m.Curve2D(points=[])
    return m.StorageUpdate(
        device_id=device_id, internal_voltage=internal_voltage,
        state_of_charge=f32(r.soc_dpct / 1000.0), available_energy=available_energy, hold_time=NAN,
        charge_time=empty_curve, discharge_time=empty_curve, max_charge_time=empty_curve,
        max_discharge_time=empty_curve, max_charge_rate=None, max_discharge_rate=None,
    )


class InverterReading:
    def __init__(self):
        self.grid_voltage_dv = 2300
        self.grid_frequency_dhz = 500
        self.ac_output_voltage_dv = 2300
        self.ac_output_frequency_dhz = 500
        self.ac_output_apparent_power_va = 5000
        self.ac_output_active_power_w = 4500
        self.battery_voltage_cv = 5100
        self.battery_charging_current_a = 400
        self.battery_discharge_current_a = 200
        self.inverter_temperature_c = 350
        self.pv_current_da = 120
        self.pv_voltage_dv = 4800
        self.pv_charging_power_w = 5760


def ac_measurement_inverter(inv: InverterReading, device_id: str, t: m.ClockMonotonic) -> m.AcMeasurementUpdate:
    out_v = f32(inv.ac_output_voltage_dv / 10.0)
    out_amp = f32(inv.ac_output_apparent_power_va / out_v) if inv.ac_output_voltage_dv > 0 else NAN
    out_line = m.AcPowerLineMeasurement(
        voltage=out_v, frequency=f32(inv.ac_output_frequency_dhz / 10.0), amperage=out_amp,
        phase_offset=None, real_power=f32(inv.ac_output_active_power_w), reactive_power=NAN,
    )
    out_port = m.AcPowerPortMeasurement(
        port_number=0, line=[out_line], phase_angle=[], real_power_rate_of_change=NAN,
        reactive_power_rate_of_change=NAN,
    )
    grid_line = m.AcPowerLineMeasurement(
        voltage=f32(inv.grid_voltage_dv / 10.0), frequency=f32(inv.grid_frequency_dhz / 10.0),
        amperage=NAN, phase_offset=None, real_power=NAN, reactive_power=NAN,
    )
    grid_port = m.AcPowerPortMeasurement(
        port_number=1, line=[grid_line], phase_angle=[], real_power_rate_of_change=NAN,
        reactive_power_rate_of_change=NAN,
    )
    return m.AcMeasurementUpdate(
        device_id=device_id, time_measured=t, external_measurement=[out_port, grid_port],
        internal_measurement=[],
    )


def dc_measurement_inverter(inv: InverterReading, device_id: str, t: m.ClockMonotonic) -> m.DcMeasurementUpdate:
    pv_voltage_cv = f32(inv.pv_voltage_dv * 10.0)
    pv_line = m.DcPowerLineMeasurement(
        voltage=f32(pv_voltage_cv / 100.0), amperage=f32(inv.pv_current_da / 10.0),
        power=f32(inv.pv_charging_power_w),
    )
    pv_port = m.DcPowerPortMeasurement(port_number=2, line=[pv_line], power_rate_of_change=NAN)
    battery_amperage = f32(inv.battery_discharge_current_a - inv.battery_charging_current_a)
    battery_voltage = f32(inv.battery_voltage_cv / 100.0)
    battery_line = m.DcPowerLineMeasurement(
        voltage=battery_voltage, amperage=battery_amperage,
        power=f32(battery_voltage * battery_amperage),
    )
    battery_port = m.DcPowerPortMeasurement(port_number=3, line=[battery_line], power_rate_of_change=NAN)
    return m.DcMeasurementUpdate(
        device_id=device_id, time_measured=t, external_measurement=[pv_port, battery_port],
        internal_measurement=[],
    )


def power_hardware_inverter(inv: InverterReading, device_id: str, t: m.ClockMonotonic) -> m.PowerHardwareUpdate:
    return m.PowerHardwareUpdate(
        device_id=device_id, timestamp=t, engine=None, fuel=None, generator=None,
        energy_storage=None,
        power_electronics=m.PowerElectronicsState(temperature=f32(inv.inverter_temperature_c)),
        thermal=None,
    )


def active_diagnostic_inverter(bits_names: set, mode_fault: bool, device_id: str, t: m.ClockMonotonic) -> m.ActiveDiagnosticState:
    codes = []
    for name, spn, hint in INVERTER_FAULT_TABLE:
        if name not in bits_names:
            continue
        codes.append(
            m.DiagnosticTroubleCode(
                port_number=None, suspect_parameter=spn,
                failure_mode=m.FailureModeIndicator.CONDITION_EXISTS, occurrence_count=1,
                time_raised=t, severity=m.DtcSeverity.MAJOR, estimate_time=None,
                estimate_severity=None, hint=hint,
            )
        )
    if mode_fault:
        codes.append(
            m.DiagnosticTroubleCode(
                port_number=None, suspect_parameter=130,
                failure_mode=m.FailureModeIndicator.CONDITION_EXISTS, occurrence_count=1,
                time_raised=t, severity=m.DtcSeverity.MAJOR, estimate_time=None,
                estimate_severity=None, hint="inverter fault mode",
            )
        )
    return m.ActiveDiagnosticState(device_id=device_id, codes=codes, overflow=0)


def device_info() -> m.DeviceInfo:
    product = m.ProductInfo(
        nsn="", gtin="", manufacturer_name="HVD", model_name="METEOR", model_number="",
        serial_number="SN-0001", software_version="0.3.0", platform_id=None,
    )
    topics = m.TopicInfo(
        data_model_version="2.0.2", published_conditional_topics=[], published_optional_topics=[],
        supported_request_topics=[], extension_topics=None,
    )
    return m.DeviceInfo(
        device_id="METEOR-001", role=m.DeviceRole.STORAGE, product=product, topics=topics,
        control_hardware=None, power_hardware=None, control_parameters=None, metric_parameters=None,
        control_service=None, power_device=None,
    )


def energy_start_stop_state() -> m.EnergyStartStopState:
    return m.EnergyStartStopState(
        device_id="METEOR-001", present_level=m.EnergyStartStopLevel.OPERATIONAL,
        future_level=m.EnergyStartStopLevel.OPERATIONAL, request_lock=False, elapsed_time=0.0,
        remaining_time=NAN, present_level_reason=m.PowerSwitchReason.STARTUP,
        present_level_actor=None, future_level_reason=m.PowerSwitchTransitionCode.NONE,
    )


def heartbeat() -> m.Heartbeat:
    return m.Heartbeat(device_id="METEOR-001", sequence_number=42)


def reply() -> m.Reply:
    status = m.ReplyStatus(code=m.ReplyCode.NOT_IMPLEMENTED, reason="nope")
    return m.Reply(
        requesting_device_id="REQ-1", target_device_id="TGT-1", config=m.ConfigId.ACTIVE,
        port_number=7, request_sequence_id=0x0102_0304_0506_0708, status=status,
    )


# Whether each golden-fixture topic's XCDR2 shape contains a sequence-of-non-primitive-element field
# ANYWHERE in its transitive member closure — CycloneDDS's own encoder (rule (12)) prefixes those with
# an extra NEXTINT length header our chain does not emit (see module doc's out-of-scope finding). Not
# a guess: derived by inspecting each topic's generated CDR chain / view for a `sequence(Struct)` or
# `sequence(str)` field.
KNOWN_SEQ_HEADER_TOPICS = {
    "device_info",  # TopicInfo's 3 (empty) string sequences
    "dc_measurement",  # external/internal_measurement: sequence(DcPowerPortMeasurement); nested line
    "power_hardware",  # energy_storage.storage_cells: sequence(StorageCellState)
    "active_diagnostic",  # codes: sequence(DiagnosticTroubleCode)
    "active_diagnostic_inverter",  # codes: sequence(DiagnosticTroubleCode)
    "storage_update",  # 4x Curve2D.points: sequence(Point2D), empty
    "ac_measurement_inverter",  # external_measurement: sequence(AcPowerPortMeasurement); nested line
    "dc_measurement_inverter",  # external_measurement: sequence(DcPowerPortMeasurement); nested line
}

FIXTURES = {}


def _build_fixtures():
    r = Reading(14, 6, -164, 5452, 775, 2)
    d = "METEOR-001"
    t = clock(12345, 678_000_000)
    inv = InverterReading()
    ti = clock(54321, 123_456_789)

    FIXTURES["heartbeat"] = heartbeat()
    FIXTURES["reply"] = reply()
    FIXTURES["device_info"] = device_info()
    FIXTURES["energy_start_stop_state"] = energy_start_stop_state()
    FIXTURES["dc_measurement"] = dc_measurement(r, d, t)
    FIXTURES["power_hardware"] = power_hardware(r, d, t)
    FIXTURES["active_diagnostic"] = active_diagnostic(0xFFFF, d, t)
    FIXTURES["storage_update"] = storage_update(r, d)
    FIXTURES["ac_measurement_inverter"] = ac_measurement_inverter(inv, "INV-001", ti)
    FIXTURES["dc_measurement_inverter"] = dc_measurement_inverter(inv, "INV-001", ti)
    FIXTURES["power_hardware_inverter"] = power_hardware_inverter(inv, "INV-001", ti)
    FIXTURES["active_diagnostic_inverter"] = active_diagnostic_inverter(
        {"line_fail", "overtemperature_fault"}, False, "INV-001", ti
    )


_build_fixtures()


# ==================================================================================================
# arbitration mode
# ==================================================================================================

# Our own committed hand-derived vectors (crates/lib-mapping/tests/wire_cdr_cross_oracle.rs) — the
# spec-literal presence-flag form lib_tms::cdr::emit_optional/emit_optional_seq currently implements.
OUR_PRESENT = bytes([
    0x00, 0x09, 0x00, 0x01,
    0x0f, 0x00, 0x00, 0x00,
    0x03, 0x00, 0x00, 0x00,
    0x4d, 0x43, 0x00,
    0x01,
    0x03, 0x00, 0x00, 0x00,
    0x4d, 0x31, 0x00,
    0x00,
])
OUR_ABSENT = bytes([
    0x00, 0x09, 0x00, 0x00,
    0x08, 0x00, 0x00, 0x00,
    0x03, 0x00, 0x00, 0x00,
    0x4d, 0x43, 0x00,
    0x00,
])


def run_arbitration() -> dict:
    report: dict = {"mode": "arbitration"}

    present = m.ActiveMicrogridControllerState(device_id="MC", master_id="M1")
    absent = m.ActiveMicrogridControllerState(device_id="MC", master_id=None)
    cyclone_present_raw = m.IdlStruct.serialize(present)
    cyclone_absent_raw = m.IdlStruct.serialize(absent)
    cyclone_present = finalize_trailing_pad(cyclone_present_raw)
    cyclone_absent = finalize_trailing_pad(cyclone_absent_raw)

    report["cyclone_present_hex"] = hexdump(cyclone_present)
    report["cyclone_absent_hex"] = hexdump(cyclone_absent)
    report["our_present_hex"] = hexdump(OUR_PRESENT)
    report["our_absent_hex"] = hexdump(OUR_ABSENT)
    report["present_matches_after_pad_fix"] = cyclone_present == OUR_PRESENT
    report["absent_matches_exactly"] = cyclone_absent == OUR_ABSENT

    # Acceptance: CycloneDDS's OWN decoder consuming our EXACT committed bytes (the strongest oracle
    # — no re-encoding round-trip involved).
    decoded_present = m.ActiveMicrogridControllerState.deserialize(OUR_PRESENT)
    decoded_absent = m.ActiveMicrogridControllerState.deserialize(OUR_ABSENT)
    report["decode_present_ok"] = (
        decoded_present.device_id == "MC" and decoded_present.master_id == "M1"
    )
    report["decode_absent_ok"] = (
        decoded_absent.device_id == "MC" and decoded_absent.master_id is None
    )
    report["decoded_present"] = repr(decoded_present)
    report["decoded_absent"] = repr(decoded_absent)

    report["verdict"] = (
        "NO_FLIP"
        if report["present_matches_after_pad_fix"]
        and report["absent_matches_exactly"]
        and report["decode_present_ok"]
        and report["decode_absent_ok"]
        else "FLIP_REQUIRED_OR_INCONCLUSIVE"
    )
    return report


# ==================================================================================================
# fixtures mode
# ==================================================================================================

def run_fixtures(compare_dir: str | None) -> dict:
    report: dict = {"mode": "fixtures", "topics": {}}
    for name, value in FIXTURES.items():
        raw = m.IdlStruct.serialize(value)
        padded = finalize_trailing_pad(raw)
        entry: dict = {"cyclone_hex": hexdump(padded), "cyclone_len": len(padded)}
        if compare_dir:
            golden_path = Path(compare_dir) / f"{name}.hex"
            if golden_path.exists():
                golden = from_hex(golden_path.read_text().strip())
                entry["golden_len"] = len(golden)
                entry["byte_equal"] = golden == padded
                if not entry["byte_equal"]:
                    entry["known_seq_header_divergence"] = name in KNOWN_SEQ_HEADER_TOPICS
                    entry["golden_hex"] = hexdump(golden)
            else:
                entry["golden_missing"] = True
        report["topics"][name] = entry
    return report


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)

    sub.add_parser("arbitration", help="dump + decode-check the @optional present/absent forms")

    fx = sub.add_parser("fixtures", help="dump all golden-fixture topics, optionally compare")
    fx.add_argument(
        "--compare", metavar="DIR", default=None,
        help=f"directory of committed *.hex goldens to diff against (e.g. {FIXTURE_DIR_DEFAULT})",
    )

    args = ap.parse_args()

    if args.cmd == "arbitration":
        report = run_arbitration()
    else:
        report = run_fixtures(args.compare)

    print(json.dumps(report, indent=2))

    if report["mode"] == "arbitration":
        return 0 if report["verdict"] == "NO_FLIP" else 1
    # fixtures mode: non-zero only on an UNEXPECTED mismatch (not the known, out-of-scope
    # sequence-header divergence, and not a missing golden — both are reported, not gate failures).
    unexpected = [
        t for t, e in report["topics"].items()
        if e.get("byte_equal") is False and not e.get("known_seq_header_divergence", False)
    ]
    if unexpected:
        print(f"UNEXPECTED mismatches (not the known seq-header divergence): {unexpected}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())

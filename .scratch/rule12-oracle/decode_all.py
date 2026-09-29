#!/usr/bin/env python3
"""Comprehensive stock-CycloneDDS decode oracle for the WP6.5 rule-(12) golden set.

Decodes each regenerated golden XCDR2 payload with the stock cyclonedds 11.0.1 CDR decoder to prove
CycloneDDS accepts our post-fix wire bytes (== vendored-patched dust_dds output). Covers the
struct-element sequences (emit_appendable_seq) AND the string-element sequences (emit_string_seq),
plus the unchanged optional-only / no-sequence topics as a regression check.
"""
import sys
sys.path.insert(0, "/probe")
import tms_mirror_types as m

# golden fixture basename -> (mirror class, was-changed-by-rule-12)
CASES = [
    ("heartbeat", m.Heartbeat, False),
    ("reply", m.Reply, False),
    ("energy_start_stop_state", m.EnergyStartStopState, False),
    ("device_info", m.DeviceInfo, True),                 # string sequences (emit_string_seq)
    ("dc_measurement", m.DcMeasurementUpdate, True),     # struct sequences (emit_appendable_seq)
    ("dc_measurement_inverter", m.DcMeasurementUpdate, True),
    ("power_hardware", m.PowerHardwareUpdate, True),
    ("active_diagnostic", m.ActiveDiagnosticState, True),
    ("active_diagnostic_inverter", m.ActiveDiagnosticState, True),
    ("storage_update", m.StorageUpdate, True),
    ("ac_measurement_inverter", m.AcMeasurementUpdate, True),
]


def load(name):
    with open(f"/data/goldens/{name}.hex") as f:
        return bytes(int(b, 16) for b in f.read().split())


ok = 0
fail = 0
for name, cls, changed in CASES:
    data = load(name)
    tag = "rule-12" if changed else "no-seq "
    try:
        s = cls.deserialize(data)
        print(f"  PASS [{tag}] {name:<26} -> {cls.__name__}.deserialize OK ({len(data)} B)")
        ok += 1
    except Exception as e:
        print(f"  FAIL [{tag}] {name:<26} -> {type(e).__name__}: {e}")
        fail += 1

print(f"\n==================== VERDICT ====================")
print(f"  stock CycloneDDS 11.0.1 decoded {ok}/{ok+fail} golden payloads; {fail} failed")
sys.exit(0 if fail == 0 else 1)

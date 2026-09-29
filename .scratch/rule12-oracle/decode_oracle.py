#!/usr/bin/env python3
"""Stock-CycloneDDS decode oracle for the XTypes rule-(12) NEXTINT fix (WP6.5).

Feeds the DcMeasurementUpdate golden XCDR2 payload to the *stock* cyclonedds 11.0.1 CDR decoder
(`IdlStruct.deserialize`) two ways:
  - NEW bytes: our chains AFTER the rule-(12) NEXTINT fix (== vendored-patched dust_dds output).
  - OLD bytes: our chains BEFORE the fix (rule-(12)-less, the shape WP7a saw CycloneDDS reject).
Proves the fix makes CycloneDDS's own decoder accept the payload, and that it rejected the old shape.
"""
import sys
sys.path.insert(0, "/probe")
from tms_mirror_types import DcMeasurementUpdate


def load(path):
    with open(path) as f:
        return bytes(int(b, 16) for b in f.read().split())


def attempt(label, path):
    data = load(path)
    print(f"\n[{label}] {len(data)} bytes: {data[:4].hex()} ...")
    try:
        sample = DcMeasurementUpdate.deserialize(data)
        n_ext = len(sample.external_measurement)
        n_int = len(sample.internal_measurement)
        print(f"  DECODE OK  device_id={sample.device_id!r} "
              f"external_measurement={n_ext} internal_measurement={n_int}")
        return True
    except Exception as e:
        print(f"  DECODE FAILED: {type(e).__name__}: {e}")
        return False


new_ok = attempt("NEW (rule-12 NEXTINT present)", "/data/dc_measurement.new.hex")
old_ok = attempt("OLD (rule-12-less, pre-fix)", "/data/dc_measurement.old.hex")

print("\n==================== VERDICT ====================")
print(f"  stock CycloneDDS 11.0.1 decodes NEW (fixed) bytes : {'PASS' if new_ok else 'FAIL'}")
print(f"  stock CycloneDDS 11.0.1 on OLD (pre-fix) bytes    : {'REJECTED (as WP7a) ' if not old_ok else 'accepted?!'}")
# Done-bar for the rule-(12) fix: NEW must decode. The OLD-rejected line reproduces the WP7a failure.
sys.exit(0 if new_ok else 1)

"""
Pure validation functions for TMS payloads against tms-compliance.md.
No I/O, no MQTT, no side effects.
"""


def validate_topic_slug(topic):
    """
    Validate that topic is exactly one of the 4 known TMS topics.

    Args:
        topic: Topic string (e.g., 'tms/METEOR-001/dc_measurement_update')

    Returns:
        List of violation strings (empty = valid)
    """
    violations = []
    known_slugs = {
        'dc_measurement_update',
        'power_hardware_update',
        'active_diagnostic_state',
        'console'  # monitor/METEOR-001/console
    }

    # Extract slug from topic
    parts = topic.split('/')
    if len(parts) >= 3:
        slug = parts[-1]
    else:
        slug = None

    if slug not in known_slugs:
        violations.append(f"Topic slug '{slug}' not in known set: {known_slugs}")

    return violations


def validate_dc_measurement_update(d):
    """
    Validate DcMeasurementUpdate payload.

    Per tms-compliance.md §3.1:
    - deviceId: string, must be "METEOR-001"
    - timeMeasured: ClockMonotonic {seconds: u32, nanoseconds: u32}
    - externalMeasurement: list, len 0..4 (adapter emits 1)
    - internalMeasurement: list, must be empty
    - Each port: port_number int, line list 1..3
    - Each line: voltage/amperage/power numeric, power_rate_of_change None or numeric

    Args:
        d: Decoded payload dict

    Returns:
        List of violation strings (empty = valid)
    """
    violations = []

    # device_id
    device_id = d.get('device_id')
    if device_id != 'METEOR-001':
        violations.append(f"device_id '{device_id}' != 'METEOR-001'")

    # time_measured
    time_measured = d.get('time_measured')
    if not isinstance(time_measured, dict):
        violations.append(f"time_measured is {type(time_measured).__name__}, not dict")
    else:
        if 'seconds' not in time_measured or not isinstance(time_measured['seconds'], int):
            violations.append("time_measured.seconds missing or not int")
        if 'nanoseconds' not in time_measured or not isinstance(time_measured['nanoseconds'], int):
            violations.append("time_measured.nanoseconds missing or not int")

    # external_measurement
    ext_meas = d.get('external_measurement')
    if not isinstance(ext_meas, list):
        violations.append(f"external_measurement is {type(ext_meas).__name__}, not list")
    else:
        if len(ext_meas) > 4:
            violations.append(f"external_measurement length {len(ext_meas)} > 4")
        for port in ext_meas:
            if not isinstance(port, dict):
                violations.append(f"external_measurement entry is {type(port).__name__}, not dict")
                continue
            # port_number
            pn = port.get('port_number')
            if not isinstance(pn, int):
                violations.append(f"external_measurement.port_number is {type(pn).__name__}, not int")
            # line
            line_list = port.get('line')
            if not isinstance(line_list, list):
                violations.append(f"external_measurement.line is {type(line_list).__name__}, not list")
            else:
                if len(line_list) < 1 or len(line_list) > 3:
                    violations.append(f"external_measurement.line length {len(line_list)} not in [1,3]")
                for i, line in enumerate(line_list):
                    if not isinstance(line, dict):
                        violations.append(f"external_measurement.line[{i}] is {type(line).__name__}, not dict")
                        continue
                    for field in ['voltage', 'amperage', 'power']:
                        val = line.get(field)
                        if not isinstance(val, (int, float)):
                            violations.append(f"external_measurement.line[{i}].{field} not numeric")
            # power_rate_of_change
            proc = port.get('power_rate_of_change')
            if proc is not None and not isinstance(proc, (int, float)):
                violations.append(f"power_rate_of_change not None or numeric (got {type(proc).__name__})")

    # internal_measurement
    int_meas = d.get('internal_measurement')
    if not isinstance(int_meas, list):
        violations.append(f"internal_measurement is {type(int_meas).__name__}, not list")
    elif len(int_meas) != 0:
        violations.append(f"internal_measurement not empty (length {len(int_meas)})")

    return violations


def validate_power_hardware_update(d):
    """
    Validate PowerHardwareUpdate payload.

    Per tms-compliance.md §3.2:
    - deviceId: "METEOR-001"
    - timestamp: ClockMonotonic
    - energyStorage: present (only populated subsystem)
      - min/average/max_temperature: numeric
      - run_time: None or numeric
      - storage_cells: list len 0..50
        - cell_number: int
        - cycle_counter: numeric
        - state_of_charge: numeric, in [0,1]
        - temperature: numeric
        - voltage: numeric
    - Keys engine/fuel/generator/powerElectronics/thermal ABSENT

    Args:
        d: Decoded payload dict

    Returns:
        List of violation strings (empty = valid)
    """
    violations = []

    # device_id
    device_id = d.get('device_id')
    if device_id != 'METEOR-001':
        violations.append(f"device_id '{device_id}' != 'METEOR-001'")

    # timestamp
    ts = d.get('timestamp')
    if not isinstance(ts, dict):
        violations.append(f"timestamp is {type(ts).__name__}, not dict")
    else:
        if 'seconds' not in ts or not isinstance(ts['seconds'], int):
            violations.append("timestamp.seconds missing or not int")
        if 'nanoseconds' not in ts or not isinstance(ts['nanoseconds'], int):
            violations.append("timestamp.nanoseconds missing or not int")

    # Forbidden keys (must be ABSENT)
    forbidden_keys = ['engine', 'fuel', 'generator', 'power_electronics', 'thermal']
    for key in forbidden_keys:
        if key in d:
            violations.append(f"Forbidden key '{key}' present (must be absent)")

    # energy_storage
    es = d.get('energy_storage')
    if es is None:
        violations.append("energy_storage missing or None")
    elif isinstance(es, dict):
        # min/average/max_temperature
        for temp_key in ['min_temperature', 'average_temperature', 'max_temperature']:
            val = es.get(temp_key)
            if not isinstance(val, (int, float)):
                violations.append(f"energy_storage.{temp_key} not numeric")

        # run_time (None or numeric)
        rt = es.get('run_time')
        if rt is not None and not isinstance(rt, (int, float)):
            violations.append(f"energy_storage.run_time not None or numeric")

        # storage_cells
        cells = es.get('storage_cells')
        if not isinstance(cells, list):
            violations.append(f"energy_storage.storage_cells is {type(cells).__name__}, not list")
        else:
            if len(cells) > 50:
                violations.append(f"energy_storage.storage_cells length {len(cells)} > 50")
            for i, cell in enumerate(cells):
                if not isinstance(cell, dict):
                    violations.append(f"storage_cells[{i}] is {type(cell).__name__}, not dict")
                    continue
                # cell_number
                cn = cell.get('cell_number')
                if not isinstance(cn, int):
                    violations.append(f"storage_cells[{i}].cell_number not int")
                # cycle_counter
                cc = cell.get('cycle_counter')
                if not isinstance(cc, (int, float)):
                    violations.append(f"storage_cells[{i}].cycle_counter not numeric")
                # state_of_charge
                soc = cell.get('state_of_charge')
                if not isinstance(soc, (int, float)):
                    violations.append(f"storage_cells[{i}].state_of_charge not numeric")
                elif not (0 <= soc <= 1):
                    violations.append(f"storage_cells[{i}].state_of_charge {soc} not in [0,1]")
                # temperature
                temp = cell.get('temperature')
                if not isinstance(temp, (int, float)):
                    violations.append(f"storage_cells[{i}].temperature not numeric")
                # voltage
                volt = cell.get('voltage')
                if not isinstance(volt, (int, float)):
                    violations.append(f"storage_cells[{i}].voltage not numeric")
    else:
        violations.append(f"energy_storage is {type(es).__name__}, not dict")

    return violations


def validate_active_diagnostic_state(d):
    """
    Validate ActiveDiagnosticState payload.

    Per tms-compliance.md §3.3:
    - deviceId: "METEOR-001"
    - codes: list len 0..64
      - suspect_parameter: int
      - failure_mode: present (str)
      - occurrence_count: int (always 1 in adapter)
      - severity: present (str, e.g., "Major")
      - hint: str len 0..32
      - time_raised: ClockMonotonic
      - port_number: None or int (always None in adapter)
    - overflow: int >= 0
    - Keys estimateTime/estimateSeverity/affectedIdentityId ABSENT

    Args:
        d: Decoded payload dict

    Returns:
        List of violation strings (empty = valid)
    """
    violations = []

    # device_id
    device_id = d.get('device_id')
    if device_id != 'METEOR-001':
        violations.append(f"device_id '{device_id}' != 'METEOR-001'")

    # Forbidden keys (must be ABSENT)
    forbidden_keys = ['estimate_time', 'estimate_severity', 'affected_identity_id']
    for key in forbidden_keys:
        if key in d:
            violations.append(f"Forbidden key '{key}' present (must be absent)")

    # codes
    codes = d.get('codes')
    if not isinstance(codes, list):
        violations.append(f"codes is {type(codes).__name__}, not list")
    else:
        if len(codes) > 64:
            violations.append(f"codes length {len(codes)} > 64")
        for i, code in enumerate(codes):
            if not isinstance(code, dict):
                violations.append(f"codes[{i}] is {type(code).__name__}, not dict")
                continue
            # suspect_parameter
            sp = code.get('suspect_parameter')
            if not isinstance(sp, int):
                violations.append(f"codes[{i}].suspect_parameter not int")
            # failure_mode
            if 'failure_mode' not in code:
                violations.append(f"codes[{i}].failure_mode missing")
            # occurrence_count
            oc = code.get('occurrence_count')
            if not isinstance(oc, int):
                violations.append(f"codes[{i}].occurrence_count not int")
            elif oc != 1:
                violations.append(f"codes[{i}].occurrence_count {oc} != 1 (adapter always emits 1)")
            # severity
            if 'severity' not in code:
                violations.append(f"codes[{i}].severity missing")
            # hint
            hint = code.get('hint')
            if not isinstance(hint, str):
                violations.append(f"codes[{i}].hint is {type(hint).__name__}, not str")
            elif len(hint) > 32:
                violations.append(f"codes[{i}].hint length {len(hint)} > 32")
            # time_raised
            tr = code.get('time_raised')
            if not isinstance(tr, dict):
                violations.append(f"codes[{i}].time_raised is {type(tr).__name__}, not dict")
            else:
                if 'seconds' not in tr or not isinstance(tr['seconds'], int):
                    violations.append(f"codes[{i}].time_raised.seconds missing or not int")
                if 'nanoseconds' not in tr or not isinstance(tr['nanoseconds'], int):
                    violations.append(f"codes[{i}].time_raised.nanoseconds missing or not int")
            # port_number
            pn = code.get('port_number')
            if pn is not None and not isinstance(pn, int):
                violations.append(f"codes[{i}].port_number not None or int")

    # overflow
    ovf = d.get('overflow')
    if not isinstance(ovf, int):
        violations.append(f"overflow is {type(ovf).__name__}, not int")
    elif ovf < 0:
        violations.append(f"overflow {ovf} < 0")

    return violations


def check_sign_inversion(dc_payload, injected_current_ca):
    """
    Check TMS sign convention: if injected current is negative (discharge),
    amperage must be > 0.

    Per tms-compliance.md §4:
    - Seplos reports positive current as charging
    - TMS positive is sourcing into the microgrid
    - So tms_amperage = -vendor_current
    - Injected negative = discharge, so amperage should be > 0

    Args:
        dc_payload: Decoded dc_measurement_update dict
        injected_current_ca: Injected current in centi-amps

    Returns:
        List of violation strings (empty = valid)
    """
    violations = []

    # Robot Framework passes literal arguments as strings (e.g. "-164"); coerce
    # to int so the numeric comparisons below don't raise TypeError (str >= int).
    injected_current_ca = int(injected_current_ca)

    if injected_current_ca >= 0:
        # Charging: no strict requirement, but amperage should be <= 0
        pass
    else:
        # Discharging: amperage must be > 0
        try:
            ext_meas = dc_payload.get('external_measurement', [])
            if len(ext_meas) > 0:
                line_list = ext_meas[0].get('line', [])
                if len(line_list) > 0:
                    amperage = line_list[0].get('amperage')
                    if amperage is not None:
                        if not (amperage > 0):
                            violations.append(
                                f"Discharge (injected_current_ca={injected_current_ca}), "
                                f"but amperage={amperage} not > 0"
                            )
                        # Optional: check magnitude
                        expected_amps = -injected_current_ca / 100.0
                        diff = abs(amperage - expected_amps)
                        if diff > 0.1:  # Allow small tolerance
                            violations.append(
                                f"Amperage {amperage} differs significantly from "
                                f"expected {expected_amps} (diff={diff})"
                            )
        except (KeyError, TypeError, IndexError):
            violations.append("Could not extract amperage from dc_payload")

    return violations


def check_nan_policy(dc_payload, hw_payload):
    """
    Check NaN policy per tms-compliance.md §4:
    - dc_payload.external_measurement[0].power_rate_of_change must be None
    - hw_payload.energy_storage.run_time must be None
    - Both payloads must be strict JSON (caller guarantees parsing)

    Args:
        dc_payload: Decoded dc_measurement_update dict
        hw_payload: Decoded power_hardware_update dict

    Returns:
        List of violation strings (empty = valid)
    """
    violations = []

    # Check power_rate_of_change is None
    try:
        proc = dc_payload.get('external_measurement', [{}])[0].get('power_rate_of_change')
        if proc is not None:
            violations.append(f"dc.external_measurement[0].power_rate_of_change is {proc}, expected None")
    except (KeyError, IndexError, TypeError):
        violations.append("Could not extract power_rate_of_change from dc_payload")

    # Check run_time is None
    try:
        rt = hw_payload.get('energy_storage', {}).get('run_time')
        if rt is not None:
            violations.append(f"hw.energy_storage.run_time is {rt}, expected None")
    except (KeyError, TypeError):
        violations.append("Could not extract run_time from hw_payload")

    return violations


if __name__ == '__main__':
    """Self-test: validate both valid and invalid payloads."""

    # Golden valid payloads (from spec)
    valid_dc = {
        "device_id": "METEOR-001",
        "time_measured": {"seconds": 1234567890, "nanoseconds": 123456789},
        "external_measurement": [{
            "port_number": 0,
            "line": [{"voltage": 54.52, "amperage": 1.64, "power": 89.4128}],
            "power_rate_of_change": None
        }],
        "internal_measurement": []
    }

    valid_hw = {
        "device_id": "METEOR-001",
        "timestamp": {"seconds": 1234567890, "nanoseconds": 123456789},
        "energy_storage": {
            "min_temperature": 20.0,
            "average_temperature": 25.0,
            "max_temperature": 30.0,
            "run_time": None,
            "storage_cells": [
                {
                    "cell_number": 0,
                    "cycle_counter": 2.0,
                    "state_of_charge": 0.775,
                    "temperature": 25.0,
                    "voltage": 3.894
                }
            ]
        }
    }

    valid_diag = {
        "device_id": "METEOR-001",
        "codes": [{
            "port_number": None,
            "suspect_parameter": 1,
            "failure_mode": "ConditionExists",
            "occurrence_count": 1,
            "time_raised": {"seconds": 1234567890, "nanoseconds": 123456789},
            "severity": "Major",
            "hint": "Test fault"
        }],
        "overflow": 0
    }

    # Broken payloads (deliberately invalid)
    broken_dc = {
        "device_id": "WRONG-ID",  # Wrong device ID
        "time_measured": {"seconds": "not_int", "nanoseconds": 123},  # Not int
        "external_measurement": [],
        "internal_measurement": [{"some": "data"}]  # Not empty
    }

    broken_hw = {
        "device_id": "METEOR-001",
        "timestamp": {"seconds": 123, "nanoseconds": 456},
        "engine": {"something": "present"},  # Forbidden key
        "energy_storage": {
            "min_temperature": "not_numeric",  # Not numeric
            "average_temperature": 25.0,
            "max_temperature": 30.0,
            "run_time": None,
            "storage_cells": [
                {"cell_number": 0, "cycle_counter": 2.0, "state_of_charge": 1.5, "temperature": 25.0, "voltage": 3.894}  # SOC out of range
            ]
        }
    }

    broken_diag = {
        "device_id": "METEOR-001",
        "estimate_time": {"seconds": 123, "nanoseconds": 456},  # Forbidden key
        "codes": [{
            "port_number": None,
            "suspect_parameter": 1,
            "failure_mode": "ConditionExists",
            "occurrence_count": 2,  # Should be 1
            "time_raised": {"seconds": 123, "nanoseconds": 456},
            "severity": "Major",
            "hint": "A" * 50  # Too long
        }],
        "overflow": -1  # Negative
    }

    # Run tests
    tests = [
        ("validate_dc_measurement_update (valid)", validate_dc_measurement_update, valid_dc, True),
        ("validate_dc_measurement_update (broken)", validate_dc_measurement_update, broken_dc, False),
        ("validate_power_hardware_update (valid)", validate_power_hardware_update, valid_hw, True),
        ("validate_power_hardware_update (broken)", validate_power_hardware_update, broken_hw, False),
        ("validate_active_diagnostic_state (valid)", validate_active_diagnostic_state, valid_diag, True),
        ("validate_active_diagnostic_state (broken)", validate_active_diagnostic_state, broken_diag, False),
        ("validate_topic_slug (valid)", validate_topic_slug, "tms/METEOR-001/dc_measurement_update", True),
        ("validate_topic_slug (broken)", validate_topic_slug, "invalid/topic", False),
        ("check_sign_inversion (discharge)", check_sign_inversion, (valid_dc, -164), True),
        ("check_nan_policy", check_nan_policy, (valid_dc, valid_hw), True),
    ]

    all_pass = True
    for test_name, func, payload, expect_valid in tests:
        if isinstance(payload, tuple):
            result = func(*payload)
        else:
            result = func(payload)

        is_valid = len(result) == 0
        status = "PASS" if is_valid == expect_valid else "FAIL"

        if status == "FAIL":
            all_pass = False
            print(f"{status}: {test_name}")
            print(f"  Expected valid={expect_valid}, got violations: {result}")
        else:
            print(f"{status}: {test_name}")

    if all_pass:
        print("\nAll tests passed!")
        exit(0)
    else:
        print("\nSome tests failed!")
        exit(1)

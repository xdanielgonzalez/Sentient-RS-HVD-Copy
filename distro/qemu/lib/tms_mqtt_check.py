#!/usr/bin/env python3
"""External stock-client proof for the Yocto guest's device-hosted MQTT broker (Task #156).

Originally modeled on the retired MCU (Zephyr) harness's mqtt_check.py (same connect/subscribe/
wait/summarize shape, same exit-code contract: 0 = pass, non-zero = fail) but genuinely reused
rather than duplicated where it counts: this script imports tms_validate.py's pure validators
VERBATIM (no re-derivation of the TMS payload schema) to assert the three payload kinds the plan
requires — DcMeasurementUpdate, PowerHardwareUpdate, ActiveDiagnosticState — by CONTENT, not just
by topic name. The MCU harness's mqtt_check.py only checked that *a* message arrived under tms/#;
this one goes further because the plan explicitly calls for validating the three TMS payload kinds.

Reachability: the Yocto guest's broker is reached through runqemu's slirp user-net `hostfwd` (see
yocto-e2e.sh's `run` subcommand) — i.e. plain `127.0.0.1:<forwarded port>` from the *container*
that ran `runqemu`, no TAP, no special routing. (The retired MCU harness instead used a
host-network TAP peer address, `192.0.2.1`.)

Usage: tms_mqtt_check.py HOST PORT client_id wait_secs
  e.g. tms_mqtt_check.py 127.0.0.1 18830 yocto-e2e-run-1 120
"""
import sys
import time
import json

import paho.mqtt.client as mqtt

# The pure TMS validators are a plain SIBLING module in this same directory (distro/qemu/lib/), so
# a bare import suffices — this script is always run by path, which puts its own directory on
# sys.path. (It used to reach cross-tree into the MCU harness via sys.path surgery; that tree was
# deleted with the Embassy pivot — docs/decisions/0010-mcu-runtime-embassy.md — and the validators
# were relocated here.) Still no copy, no re-derivation of the TMS schema: e2e_common.resource
# loads this very same file as a Robot Library.
import tms_validate

TMS_KIND_BY_SLUG = {
    "dc_measurement_update": "DcMeasurementUpdate",
    "power_hardware_update": "PowerHardwareUpdate",
    "active_diagnostic_state": "ActiveDiagnosticState",
}
VALIDATOR_BY_SLUG = {
    "dc_measurement_update": tms_validate.validate_dc_measurement_update,
    "power_hardware_update": tms_validate.validate_power_hardware_update,
    "active_diagnostic_state": tms_validate.validate_active_diagnostic_state,
}

received = {}


def on_connect(client, userdata, flags, reason_code, properties=None):
    print(f"[client] CONNACK reason_code={reason_code}", flush=True)
    client.subscribe("tms/#", qos=0)
    client.subscribe("monitor/#", qos=0)


def on_message(client, userdata, msg):
    print(f"[client] RECV topic={msg.topic!r} retain={msg.retain} payload={msg.payload[:300]!r}", flush=True)
    received[msg.topic] = (msg.retain, msg.payload)


def slug_of(topic):
    parts = topic.split("/")
    return parts[-1] if len(parts) >= 3 else None


def main():
    if len(sys.argv) < 3:
        print(f"usage: {sys.argv[0]} HOST PORT [client_id] [wait_secs]", file=sys.stderr)
        return 2
    host = sys.argv[1]
    port = int(sys.argv[2])
    client_id = sys.argv[3] if len(sys.argv) > 3 else "yocto-e2e-external-check"
    wait_secs = int(sys.argv[4]) if len(sys.argv) > 4 else 120

    client = mqtt.Client(mqtt.CallbackAPIVersion.VERSION2, client_id=client_id)
    client.on_connect = on_connect
    client.on_message = on_message

    connect_deadline = time.time() + 60
    connected = False
    while time.time() < connect_deadline and not connected:
        try:
            client.connect(host, port, keepalive=30)
            connected = True
        except OSError as e:
            print(f"[client] connect failed ({e}), retrying...", flush=True)
            time.sleep(2)

    if not connected:
        print(f"[client] FAIL: could not connect to {host}:{port} within deadline")
        return 1

    client.loop_start()

    # Wait until we've seen all three required TMS slugs (or the deadline expires). Unlike
    # mqtt_check.py's has_tms/has_monitor OR-check, this requires ALL THREE kinds per the plan's
    # explicit assertion list.
    required_slugs = set(TMS_KIND_BY_SLUG)
    wait_deadline = time.time() + wait_secs
    while time.time() < wait_deadline:
        seen_slugs = {slug_of(t) for t in received if t.startswith("tms/")}
        if required_slugs <= seen_slugs:
            break
        time.sleep(1)

    client.loop_stop()
    client.disconnect()

    print("\n[client] === SUMMARY ===")
    for topic, (retain, payload) in sorted(received.items()):
        print(f"  {topic}: retain={retain} len={len(payload)}")

    seen_slugs = {slug_of(t) for t in received if t.startswith("tms/")}
    missing = required_slugs - seen_slugs
    if missing:
        print(f"[client] FAIL: missing TMS payload kinds: {sorted(TMS_KIND_BY_SLUG[m] for m in missing)}")
        return 1

    # Content-validate each of the three required payloads with tms_validate's own rules (reused,
    # not re-derived). A topic present but a schema-invalid payload is still a FAIL.
    all_violations = []
    for topic, (_, payload) in received.items():
        slug = slug_of(topic)
        validator = VALIDATOR_BY_SLUG.get(slug)
        if validator is None:
            continue
        try:
            decoded = json.loads(payload)
        except (json.JSONDecodeError, UnicodeDecodeError) as e:
            all_violations.append(f"{topic}: payload is not valid JSON ({e})")
            continue
        violations = validator(decoded)
        if violations:
            all_violations.append(f"{topic}: {violations}")
        else:
            print(f"[client] PASS: {TMS_KIND_BY_SLUG[slug]} ({topic}) schema-valid")

    if all_violations:
        print("[client] FAIL: schema violations:")
        for v in all_violations:
            print(f"  - {v}")
        return 1

    print("[client] PASS: all three TMS payload kinds present and schema-valid "
          "(DcMeasurementUpdate, PowerHardwareUpdate, ActiveDiagnosticState)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

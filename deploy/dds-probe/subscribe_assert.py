#!/usr/bin/env python3
"""subscribe_assert.py — the containerized CycloneDDS interop-assertion probe (story #321 / #323).

Joins a MIL-STD-3071 DDS domain (default 0) as a CycloneDDS participant, subscribes one or more
named TMS topics, and asserts the interop done-bar from the DDS/RTPS transport plan
(`logical-brewing-salamander.md`): sample receipt, USER_DATA == the publishing device's identity,
type decode (a well-formed sample of the expected mirror type — see `tms_mirror_types.py`),
late-joiner TRANSIENT_LOCAL replay, and Reply KEEP_LAST-depth semantics. Exits non-zero (with a
machine-readable JSON report on stdout) on any assertion failure — this is the oracle a CI lane
gates on, not a human-read log.

CycloneDDS's `CYCLONEDDS_URI` environment variable controls the participant config (network
interface allow-list, IPv6-only via `General/Interfaces/NetworkInterface[@address=".."][@config=..]`
or `General/Transport=udp6`, etc.) — this script deliberately does NOT hardcode transport config; the
caller sets `CYCLONEDDS_URI` (see `test-e2e/tests/dds_interop_e2e.rs`, WP7) so the SAME probe image
serves both the same-netns CI lane and an external-participant smoke run
(`SENTIENT_TEST_DDS_ENDPOINT`).

Usage:
    subscribe_assert.py --topic Heartbeat --domain 0 --timeout 20 \\
        [--expect-user-data METEOR-001] [--expect-samples 1] \\
        [--assert-transient-local] [--assert-keep-last N] [--late-joiner-delay SECONDS]
"""
from __future__ import annotations

import argparse
import json
import sys
import time

from cyclonedds.core import Listener, Qos, Policy
from cyclonedds.domain import DomainParticipant
from cyclonedds.sub import DataReader
from cyclonedds.topic import Topic
from cyclonedds.util import duration

import tms_mirror_types as m

# Topic name -> mirror type + a stable field name to read the instance/USER_DATA-correlated key from
# (CycloneDDS's Python binding surfaces publisher USER_DATA via `sample_info`/builtin topics, not the
# sample itself — see `_read_user_data` below; the per-sample field is used for a same-value sanity
# check when the topic's key IS the device id, which is every TMS topic here).
TOPIC_TYPES = {
    "Heartbeat": m.Heartbeat,
    "Reply": m.Reply,
    "DeviceInfo": m.DeviceInfo,
    "EnergyStartStopState": m.EnergyStartStopState,
    "DcMeasurementUpdate": m.DcMeasurementUpdate,
    "PowerHardwareUpdate": m.PowerHardwareUpdate,
    "ActiveDiagnosticState": m.ActiveDiagnosticState,
    "StorageUpdate": m.StorageUpdate,
    "AcMeasurementUpdate": m.AcMeasurementUpdate,
    "ActiveMicrogridControllerState": m.ActiveMicrogridControllerState,
}


# MIL-STD-3071 Annex D.5 — the 8 named QoS profiles as (transient_local, reliable, history_depth).
# A verbatim mirror of `lib_tms::generated_qos::QosProfile::policies` (the writer side). The probe
# reads each topic with its OWN profile QoS because a DataReader must be RxO-compatible with the
# matched writer: a RELIABLE reader silently fails to match a BEST_EFFORT writer, and a
# TRANSIENT_LOCAL reader fails to match a VOLATILE writer. The prior single Reliable/Volatile reader
# therefore never received the BestEffort telemetry topics (Medium/Slow/Continuous) — the WP7a
# zero-delivery blocker this fixes.
PROFILES = {
    "PublishLast": dict(transient_local=True, reliable=True, depth=1),
    "Rare": dict(transient_local=False, reliable=False, depth=1),
    "Slow": dict(transient_local=False, reliable=False, depth=1),
    "Medium": dict(transient_local=False, reliable=False, depth=1),
    "Continuous": dict(transient_local=False, reliable=False, depth=1),
    "Command": dict(transient_local=False, reliable=True, depth=1),
    "Response": dict(transient_local=False, reliable=True, depth=1),
    "Reply": dict(transient_local=False, reliable=True, depth=128),
}

# Topic -> its D.5 profile (mirror of `lib_tms::generated_qos::qos_profile` for the subset this probe
# subscribes). Telemetry (Measurement*/Update) = Medium/Slow (BestEffort/Volatile); the *State topics
# and DeviceInfo = PublishLast (TransientLocal/Reliable, late-joiner replay); Reply = Reply
# (Reliable, KEEP_LAST 128).
TOPIC_PROFILES = {
    "Heartbeat": "Medium",
    "Reply": "Reply",
    "DeviceInfo": "PublishLast",
    "EnergyStartStopState": "PublishLast",
    "DcMeasurementUpdate": "Medium",
    "PowerHardwareUpdate": "Slow",
    "ActiveDiagnosticState": "PublishLast",
    "StorageUpdate": "Slow",
    "AcMeasurementUpdate": "Medium",
    "ActiveMicrogridControllerState": "PublishLast",
}


def _profile_qos(profile: str, override_depth: int | None = None) -> Qos:
    """Build the reader `Qos` for a D.5 `profile`, RxO-matching that profile's writer. `override_depth`
    lets `--assert-keep-last N` deepen the reader history (e.g. to observe Reply's KEEP_LAST 128)
    without changing reliability/durability, which MUST stay profile-matched for the match to form."""
    spec = PROFILES[profile]
    depth = override_depth if override_depth else spec["depth"]
    reliability = (
        Policy.Reliability.Reliable(max_blocking_time=duration(seconds=2))
        if spec["reliable"]
        else Policy.Reliability.BestEffort
    )
    durability = (
        Policy.Durability.TransientLocal if spec["transient_local"] else Policy.Durability.Volatile
    )
    return Qos(reliability, durability, Policy.History.KeepLast(depth))


def _read_user_data(reader: DataReader) -> list[bytes]:
    """Best-effort USER_DATA extraction from the matched writer(s)' discovered
    `PublicationBuiltinTopicData` — the TMS contract is `USER_DATA = deviceId` on every writer
    (MIL-STD-3071; see `lib-dds`'s `DdsParticipant::create_writer`). Returns raw bytes per matched
    writer; empty if none matched yet (a discovery-timing condition the caller's timeout covers)."""
    try:
        matched = reader.get_matched_publications()
    except Exception:  # pragma: no cover - binding-version guard, not a control-flow path
        return []
    out = []
    for pub_handle in matched or []:
        try:
            data = reader.get_matched_publication_data(pub_handle)
            ud = getattr(data, "user_data", None)
            if ud:
                out.append(bytes(ud))
        except Exception:  # pragma: no cover
            continue
    return out


def run(args: argparse.Namespace) -> dict:
    report: dict = {
        "topic": args.topic, "domain": args.domain, "assertions": {}, "samples_received": 0,
    }
    idl_type = TOPIC_TYPES.get(args.topic)
    if idl_type is None:
        report["error"] = f"unknown topic {args.topic!r}; known: {sorted(TOPIC_TYPES)}"
        return report

    dp = DomainParticipant(args.domain)
    tp = Topic(dp, args.topic, idl_type)
    profile = args.profile or TOPIC_PROFILES.get(args.topic, "Medium")
    report["profile"] = profile
    qos = _profile_qos(profile, override_depth=args.assert_keep_last or None)

    if args.late_joiner_delay > 0:
        # Prove TRANSIENT_LOCAL replay: create the reader only AFTER the delay, so any sample
        # published before this point can only reach us via durability replay, not live delivery.
        time.sleep(args.late_joiner_delay)

    rd = DataReader(dp, tp, qos=qos)

    deadline = time.time() + args.timeout
    samples = []
    while time.time() < deadline and len(samples) < args.expect_samples:
        for s in rd.take(N=max(1, args.assert_keep_last or 1)):
            samples.append(s)
        if len(samples) < args.expect_samples:
            time.sleep(0.2)

    report["samples_received"] = len(samples)
    report["assertions"]["sample_received"] = len(samples) > 0
    report["assertions"]["expected_count_met"] = len(samples) >= args.expect_samples
    report["assertions"]["type_decode_ok"] = all(isinstance(s, idl_type) for s in samples)

    if args.assert_keep_last:
        report["assertions"]["keep_last_depth_respected"] = len(samples) <= args.assert_keep_last

    if args.expect_user_data:
        user_data_values = _read_user_data(rd)
        decoded = [ud.decode("utf-8", errors="replace") for ud in user_data_values]
        report["user_data_seen"] = decoded
        report["assertions"]["user_data_matches"] = args.expect_user_data in decoded

    if args.assert_transient_local:
        # A TRANSIENT_LOCAL sample received despite the reader being created AFTER
        # `late_joiner_delay` proves durability replay (not live delivery racing the sleep).
        report["assertions"]["transient_local_replay"] = (
            args.late_joiner_delay > 0 and len(samples) > 0
        )

    if samples:
        report["first_sample"] = repr(samples[0])

    ok = all(report["assertions"].values()) if report["assertions"] else False
    report["ok"] = ok
    return report


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--topic", required=True, choices=sorted(TOPIC_TYPES))
    ap.add_argument(
        "--profile", default=None, choices=sorted(PROFILES),
        help="override the reader's D.5 QoS profile (default: the topic's own profile, so the "
        "reader is RxO-compatible with its writer)",
    )
    ap.add_argument("--domain", type=int, default=0)
    ap.add_argument("--timeout", type=float, default=20.0, help="seconds to wait for samples")
    ap.add_argument("--expect-samples", type=int, default=1)
    ap.add_argument("--expect-user-data", default=None, help="deviceId expected in a writer's USER_DATA")
    ap.add_argument("--assert-transient-local", action="store_true")
    ap.add_argument("--assert-keep-last", type=int, default=0, metavar="N")
    ap.add_argument(
        "--late-joiner-delay", type=float, default=0.0, metavar="SECONDS",
        help="wait this long BEFORE creating the reader, to prove TRANSIENT_LOCAL replay",
    )
    args = ap.parse_args()

    report = run(args)
    print(json.dumps(report, indent=2))
    return 0 if report.get("ok") else 1


if __name__ == "__main__":
    sys.exit(main())

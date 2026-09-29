#!/usr/bin/env python3
"""local_delivery_probe.py — the DATA-delivery isolation oracle (WP7a, check (d)).

A CycloneDDS subscriber that provides the topic type LOCALLY (IdlStruct mirrors of the dust_dds
mirror types), so it does NOT depend on XTypes TypeInformation propagation the way `cyclonedds
subscribe`/`typeof` do. This cleanly separates the two possible causes of a stock-`subscribe` miss:

  * this probe RECEIVES  -> the RTPS DATA path works; the stock-CLI miss is the XTypes type-info gate.
  * this probe is SILENT -> DATA never arrives; the dust_dds RTPS DATA-delivery path is the blocker
                            (the WP6 zero-delivery datum, confirmed with a matching-QoS reader).

QoS MUST match (or be weaker than) the writer's offered QoS or SEDP endpoint matching silently fails
even though participant discovery succeeds. The published TMS telemetry writers use QosProfile::Medium
= BestEffort / Volatile / KeepLast(1) (lib_tms::generated_qos), so this reader offers exactly that —
unlike deploy/dds-probe/subscribe_assert.py, which requests Reliable and therefore will NOT match a
BestEffort writer. Type shapes mirror lib-dds/src/dustdds/types/mirror/{heartbeat,
dc_measurement_update,...}.rs 1:1 (APPENDABLE, same field order).

Usage: local_delivery_probe.py --topic Heartbeat [--topic DcMeasurementUpdate] --domain 0 --timeout 15
Exit 0 iff every requested topic delivered >=1 sample; nonzero otherwise (JSON report on stdout).

NOTE: do NOT add `from __future__ import annotations`. cyclonedds 11.0.1's IDL normalizer resolves
stringized annotations via getattr(<defining module>, name); with PEP 563 stringization + IdlStruct
classes defined in __main__, `device_id: str` becomes getattr(__main__, "str") -> AttributeError.
Real (non-stringized) annotations avoid that path entirely.
"""
import argparse
import json
import sys
import time
from dataclasses import dataclass

from cyclonedds.core import Policy, Qos
from cyclonedds.domain import DomainParticipant
from cyclonedds.idl import IdlStruct
from cyclonedds.idl.annotations import appendable, key
from cyclonedds.idl.types import float32, int32, sequence, uint32
from cyclonedds.sub import DataReader
from cyclonedds.topic import Topic


@appendable
@dataclass
class Heartbeat(IdlStruct, typename="Heartbeat"):
    device_id: str
    key("device_id")
    sequence_number: uint32


@appendable
@dataclass
class ClockMonotonic(IdlStruct, typename="ClockMonotonic"):
    seconds: uint32
    nanoseconds: uint32


@appendable
@dataclass
class DcPowerLineMeasurement(IdlStruct, typename="DcPowerLineMeasurement"):
    voltage: float32
    amperage: float32
    power: float32


@appendable
@dataclass
class DcPowerPortMeasurement(IdlStruct, typename="DcPowerPortMeasurement"):
    port_number: int32
    line: sequence[DcPowerLineMeasurement]
    power_rate_of_change: float32


@appendable
@dataclass
class DcMeasurementUpdate(IdlStruct, typename="DcMeasurementUpdate"):
    device_id: str
    key("device_id")
    time_measured: ClockMonotonic
    external_measurement: sequence[DcPowerPortMeasurement]
    internal_measurement: sequence[DcPowerPortMeasurement]


TOPIC_TYPES = {
    "Heartbeat": Heartbeat,
    "DcMeasurementUpdate": DcMeasurementUpdate,
}


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--topic", action="append", required=True, choices=sorted(TOPIC_TYPES))
    ap.add_argument("--domain", type=int, default=0)
    ap.add_argument("--timeout", type=float, default=15.0)
    args = ap.parse_args()

    dp = DomainParticipant(args.domain)
    # BestEffort/Volatile/KeepLast(1): matches the Medium-profile telemetry writers.
    qos = Qos(Policy.Reliability.BestEffort, Policy.Durability.Volatile, Policy.History.KeepLast(1))
    readers = {t: DataReader(dp, Topic(dp, t, TOPIC_TYPES[t]), qos=qos) for t in args.topic}

    report: dict = {"domain": args.domain, "topics": {}, "matched": {}}
    got: dict = {t: None for t in args.topic}
    deadline = time.time() + args.timeout
    while time.time() < deadline and any(v is None for v in got.values()):
        for t, rd in readers.items():
            if got[t] is not None:
                continue
            try:
                report["matched"][t] = len(rd.get_matched_publications() or [])
            except Exception:
                pass
            for s in rd.take(N=20):
                got[t] = repr(s)
                break
        if any(v is None for v in got.values()):
            time.sleep(0.2)

    ok = True
    for t in args.topic:
        received = got[t] is not None
        report["topics"][t] = {"received": received, "sample": got[t]}
        ok = ok and received
    report["ok"] = ok
    print(json.dumps(report, indent=2))
    return 0 if ok else 2


if __name__ == "__main__":
    sys.exit(main())

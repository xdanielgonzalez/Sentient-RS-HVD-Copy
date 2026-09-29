#!/usr/bin/env python3
"""Advisory hotpath bench-metrics gate (Slice C — `just bench-check` / `just bench-baseline`).

Parses a hotpath (crate `hotpath` v0.21) JSON report, compares per-function timing/alloc metrics
against `.ci/scripts/bench_baseline.json`, and renders a markdown TREND table into
$GITHUB_STEP_SUMMARY (or stdout when that's unset — e.g. a local `just bench-check`).

This gate is ADVISORY by design: it never fails a build on its own. `--check` compares + prints but
still exits 0; only `--check --strict` can exit 1, and only when a function's timing metric blows
past `--fail-ratio` (default 3.0 — a generous ceiling meant to catch an algorithmic blowup, not
normal run-to-run jitter).

Modes:
  (default)          parse + render the trend table (no baseline comparison gating).
  --check            also compute baseline-ratio breaches and list them; still exit 0 unless --strict.
  --check --strict   exit 1 if any function's gate metric (avg, falling back to p99, then total)
                     exceeds baseline * --fail-ratio.
  --write-baseline   write the current run's extracted metrics as the new baseline; exit 0.

Usage:
  bench_metrics_audit.py [--input PATH] [--baseline PATH] [--check] [--strict]
                          [--fail-ratio N] [--write-baseline]
"""
from __future__ import annotations

import argparse
import json
import os
import re
import sys
from pathlib import Path

DEFAULT_INPUT = Path("target/hotpath/bench.json")
BASELINE = Path(".ci/scripts/bench_baseline.json")

# ---- hotpath JSON key catalogue ------------------------------------------------------------------
# Verified against a LIVE hotpath 0.21.5 `#[hotpath::main(format = "json-pretty")]` run: the report
# is `{"functions_timing": {"data": [...]}, "functions_alloc": {"data": [...]}, "functions_cpu": ...,
# "threads": {...}}`; each `data[]` entry is `{"id", "name", "calls", "sampled_calls", "avg", "p50",
# "p95", "p99", "total", "percent_total"}` with metric values as HUMAN UNIT-STRINGS ("51 ns",
# "315.19 ms", "500 B", "1.7 KB") that must be normalized before comparison.
#
# TODO(orchestrator): re-verify field names against `crates/bin/rpi/examples/hotpath_e2e.rs`'s
# actual emitted JSON once Track A lands (a different hotpath point-release could rename a field).
# Kept as tried-key-list CONSTANTS below (not inlined literals) so a rename is a one-line fix here,
# not a parser rewrite. Aliases beyond the single verified key per field are defensive extras, not
# independently confirmed against a live run.
TIMING_BLOCK_KEYS = ("functions_timing",)      # top-level key: the per-fn TIMING block
ALLOC_BLOCK_KEYS = ("functions_alloc",)        # top-level key: the per-fn ALLOC-BYTES block
DATA_LIST_KEYS = ("data",)                     # entries list nested within a block

NAME_KEYS = ("name", "function")
CALLS_KEYS = ("calls", "call_count", "count")
SAMPLED_CALLS_KEYS = ("sampled_calls",)
AVG_KEYS = ("avg", "mean")
P50_KEYS = ("p50",)
P95_KEYS = ("p95",)
P99_KEYS = ("p99",)
TOTAL_KEYS = ("total", "sum")

# Fallback flat shape for --input / --baseline: a top-level list, or {"functions"|"entries": [...]}
# of ALREADY-canonical (pre-normalized, numeric) records — our own bench_baseline.json shape, or a
# hypothetical future hotpath version that flattens the report. Never hit against a real hotpath
# 0.21 `functions_timing`/`functions_alloc` report.
FLAT_FUNCTIONS_KEYS = ("functions", "entries")

_TIME_UNIT_NS = {
    "ns": 1.0,
    "us": 1_000.0,
    "µs": 1_000.0,  # µs
    "ms": 1_000_000.0,
    "s": 1_000_000_000.0,
}

# TODO(orchestrator): hotpath's byte-unit base (1024 vs 1000) isn't documented; inferred empirically
# from a live sample (`hp.json`, a throwaway hotpath probe harness) where avg*calls ~= total only
# reconciles under KB=1024 — e.g. "hpprobe::work": avg="500 B", calls=2000, total="977.2 KB" =>
# 500*2000/1024 = 976.6 KB ~= 977.2 KB (the KB=1000 reading is off by ~2.3%). NOTE the --fail-ratio
# gate is base-INVARIANT: a wrong base scales baseline and current identically, so only the printed
# absolute avg/total columns would be mislabeled, never the pass/fail decision. Flip BYTES_UNIT_BASE
# to 1000.0 if this is ever contradicted by hotpath's own docs/source.
BYTES_UNIT_BASE = 1024.0
_BYTE_UNIT_MULTIPLIER = {
    "b": 1.0,
    "kb": BYTES_UNIT_BASE,
    "mb": BYTES_UNIT_BASE**2,
    "gb": BYTES_UNIT_BASE**3,
}

# The single metric --strict compares against baseline*ratio (the "algorithmic blowup" catch).
# Preference order: avg (per-call cost) is the most meaningful "did this get slower" signal; fall
# back to p99 then total if a function's avg wasn't emitted (e.g. only one profiling mode ran).
GATE_METRIC_PREFERENCE = ("avg_ns", "p99_ns", "total_ns")
_GATE_METRIC_LABEL = {"avg_ns": "avg", "p99_ns": "p99", "total_ns": "total"}

_UNIT_RE = re.compile(r"^\s*([0-9]+(?:\.[0-9]+)?)\s*([a-zA-Zµ]+)\s*$")


def _get_any(d: dict, keys: tuple) -> object:
    """First present key's value from `keys`, tried in order. None if none present."""
    for k in keys:
        if k in d:
            return d[k]
    return None


def parse_unit_string(value: object, table: dict) -> float | None:
    """'315.19 ms' -> 315190000.0 given the ns table; '500 B' -> 500.0 given the byte table.
    Accepts an already-numeric value passthrough. Never raises — returns None on anything that
    doesn't parse (missing key, unrecognized unit, malformed number)."""
    if isinstance(value, bool):
        return None
    if isinstance(value, (int, float)):
        return float(value)
    if not isinstance(value, str):
        return None
    m = _UNIT_RE.match(value)
    if not m:
        return None
    num, unit = m.groups()
    mult = table.get(unit.lower())
    if mult is None:
        return None
    try:
        return float(num) * mult
    except ValueError:
        return None


def _num(entry: dict, keys: tuple, unit_table: dict | None = None) -> float | None:
    v = _get_any(entry, keys)
    if v is None:
        return None
    if unit_table is not None:
        return parse_unit_string(v, unit_table)
    if isinstance(v, bool):
        return None
    if isinstance(v, (int, float)):
        return float(v)
    if isinstance(v, str):
        try:
            return float(v)
        except ValueError:
            return None
    return None


def _name_of(entry: dict) -> str | None:
    v = _get_any(entry, NAME_KEYS)
    return v if isinstance(v, str) and v else None


def blank_record(name: str) -> dict:
    return {
        "name": name,
        "calls": None,
        "sampled_calls": None,
        "avg_ns": None,
        "p50_ns": None,
        "p95_ns": None,
        "p99_ns": None,
        "total_ns": None,
        "alloc_avg_bytes": None,
        "alloc_p50_bytes": None,
        "alloc_p95_bytes": None,
        "alloc_p99_bytes": None,
        "alloc_total_bytes": None,
    }


def _block_entries(top: dict, block_keys: tuple) -> list:
    block = _get_any(top, block_keys)
    if isinstance(block, list):
        return [e for e in block if isinstance(e, dict)]
    if isinstance(block, dict):
        data = _get_any(block, DATA_LIST_KEYS)
        if isinstance(data, list):
            return [e for e in data if isinstance(e, dict)]
    return []


def load_report(path: Path) -> dict:
    """Load+parse a JSON report. Returns {} on any missing/unreadable/malformed file — never
    crashes (a missing bench.json — e.g. hotpath-alloc off — is a valid, silent no-op run)."""
    if not path.exists():
        return {}
    try:
        with path.open(encoding="utf-8") as fh:
            return json.load(fh)
    except (OSError, json.JSONDecodeError, ValueError) as exc:
        print(f"warn: could not read/parse {path}: {exc}", file=sys.stderr)
        return {}


def extract_current(top: object) -> dict:
    """Merge functions_timing + functions_alloc blocks into one canonical per-function record
    dict, keyed by function name, with metrics normalized to ns / bytes. Tolerates either block
    being absent (e.g. hotpath-alloc feature off leaves functions_alloc empty/missing)."""
    if not isinstance(top, dict):
        return {}

    records: dict = {}

    for entry in _block_entries(top, TIMING_BLOCK_KEYS):
        name = _name_of(entry)
        if not name:
            continue
        rec = records.setdefault(name, blank_record(name))
        calls = _num(entry, CALLS_KEYS)
        if calls is not None:
            rec["calls"] = calls
        rec["sampled_calls"] = _num(entry, SAMPLED_CALLS_KEYS)
        rec["avg_ns"] = _num(entry, AVG_KEYS, _TIME_UNIT_NS)
        rec["p50_ns"] = _num(entry, P50_KEYS, _TIME_UNIT_NS)
        rec["p95_ns"] = _num(entry, P95_KEYS, _TIME_UNIT_NS)
        rec["p99_ns"] = _num(entry, P99_KEYS, _TIME_UNIT_NS)
        rec["total_ns"] = _num(entry, TOTAL_KEYS, _TIME_UNIT_NS)

    for entry in _block_entries(top, ALLOC_BLOCK_KEYS):
        name = _name_of(entry)
        if not name:
            continue
        rec = records.setdefault(name, blank_record(name))
        if rec["calls"] is None:
            rec["calls"] = _num(entry, CALLS_KEYS)
        rec["alloc_avg_bytes"] = _num(entry, AVG_KEYS, _BYTE_UNIT_MULTIPLIER)
        rec["alloc_p50_bytes"] = _num(entry, P50_KEYS, _BYTE_UNIT_MULTIPLIER)
        rec["alloc_p95_bytes"] = _num(entry, P95_KEYS, _BYTE_UNIT_MULTIPLIER)
        rec["alloc_p99_bytes"] = _num(entry, P99_KEYS, _BYTE_UNIT_MULTIPLIER)
        rec["alloc_total_bytes"] = _num(entry, TOTAL_KEYS, _BYTE_UNIT_MULTIPLIER)

    if records:
        return records

    # Tolerant fallback: an already-canonical flat shape (top-level list, or
    # {"functions"|"entries": [...]}) of numeric (non-unit-string) records — our own
    # bench_baseline.json round-trips through this path too.
    flat = top if isinstance(top, list) else _get_any(top, FLAT_FUNCTIONS_KEYS)
    if isinstance(flat, list):
        for entry in flat:
            if not isinstance(entry, dict):
                continue
            name = _name_of(entry)
            if not name:
                continue
            rec = blank_record(name)
            for field in rec:
                if field != "name" and field in entry:
                    v = entry[field]
                    if isinstance(v, (int, float)) and not isinstance(v, bool):
                        rec[field] = float(v)
            records[name] = rec

    return records


def gate_value(rec: dict):
    """(field, value) for the first populated metric in GATE_METRIC_PREFERENCE, or (None, None)."""
    for field in GATE_METRIC_PREFERENCE:
        v = rec.get(field)
        if v is not None:
            return field, v
    return None, None


def compute_breaches(current: dict, baseline: dict, fail_ratio: float) -> list:
    breaches = []
    for name, rec in current.items():
        base = baseline.get(name)
        if not base:
            continue
        field, cur_val = gate_value(rec)
        if field is None:
            continue
        base_val = base.get(field)
        if base_val is None or base_val <= 0:
            continue
        ratio = cur_val / base_val
        if ratio > fail_ratio:
            breaches.append((name, field, base_val, cur_val, ratio))
    breaches.sort(key=lambda b: -b[4])
    return breaches


def fmt_ns(ns) -> str:
    if ns is None:
        return "—"
    if ns < 1_000:
        return f"{ns:.0f} ns"
    if ns < 1_000_000:
        return f"{ns / 1_000:.2f} µs"
    if ns < 1_000_000_000:
        return f"{ns / 1_000_000:.2f} ms"
    return f"{ns / 1_000_000_000:.2f} s"


def fmt_bytes(b) -> str:
    if b is None:
        return "—"
    if b < BYTES_UNIT_BASE:
        return f"{b:.0f} B"
    if b < BYTES_UNIT_BASE**2:
        return f"{b / BYTES_UNIT_BASE:.1f} KB"
    if b < BYTES_UNIT_BASE**3:
        return f"{b / BYTES_UNIT_BASE**2:.1f} MB"
    return f"{b / BYTES_UNIT_BASE**3:.2f} GB"


def fmt_count(n) -> str:
    if n is None:
        return "—"
    return f"{n:.0f}" if float(n).is_integer() else f"{n:g}"


def ratio_cell(rec: dict, base: dict | None) -> str:
    if not base:
        return "new"
    field, cur_val = gate_value(rec)
    if field is None:
        return "—"
    base_val = base.get(field)
    if base_val is None or base_val <= 0:
        return "—"
    ratio = cur_val / base_val
    label = _GATE_METRIC_LABEL[field]
    flag = " ⚠" if ratio > 1.0 else ""
    return f"{ratio:.2f}x ({label}){flag}"


def render_summary(current: dict, baseline: dict, fail_ratio: float, checked: bool, breaches: list) -> str:
    lines = ["### hotpath bench trend (advisory)", ""]
    if not current:
        lines.append("_no functions parsed from the hotpath JSON report — nothing to compare._")
        return "\n".join(lines)

    lines.append("| function | calls | avg | p50 | p95 | p99 | total | alloc avg | alloc total | vs baseline |")
    lines.append("|---|---|---|---|---|---|---|---|---|---|")
    for name in sorted(current):
        rec = current[name]
        base = baseline.get(name)
        lines.append(
            "| "
            + " | ".join(
                [
                    f"`{name}`",
                    fmt_count(rec.get("calls")),
                    fmt_ns(rec.get("avg_ns")),
                    fmt_ns(rec.get("p50_ns")),
                    fmt_ns(rec.get("p95_ns")),
                    fmt_ns(rec.get("p99_ns")),
                    fmt_ns(rec.get("total_ns")),
                    fmt_bytes(rec.get("alloc_avg_bytes")),
                    fmt_bytes(rec.get("alloc_total_bytes")),
                    ratio_cell(rec, base),
                ]
            )
            + " |"
        )

    if checked:
        lines.append("")
        if breaches:
            lines.append(
                f"**{len(breaches)} function(s) exceed {fail_ratio:g}x baseline** "
                f"(advisory unless run with `--strict`):"
            )
            for name, field, base_val, cur_val, ratio in breaches:
                label = _GATE_METRIC_LABEL[field]
                lines.append(f"- `{name}` {label}: {ratio:.2f}x baseline ({fmt_ns(base_val)} → {fmt_ns(cur_val)})")
        else:
            lines.append(f"No function exceeds {fail_ratio:g}x baseline.")

    return "\n".join(lines)


def write_summary(text: str) -> None:
    dest = os.environ.get("GITHUB_STEP_SUMMARY")
    if dest:
        with open(dest, "a", encoding="utf-8") as fh:
            fh.write(text + "\n")
    else:
        print(text)


def write_baseline(path: Path, current: dict) -> None:
    payload = {"functions": [current[name] for name in sorted(current)]}
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as fh:
        json.dump(payload, fh, indent=2)
        fh.write("\n")


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--input", type=Path, default=DEFAULT_INPUT, help=f"hotpath JSON report (default {DEFAULT_INPUT})")
    ap.add_argument("--baseline", type=Path, default=BASELINE, help=f"baseline JSON (default {BASELINE})")
    ap.add_argument("--check", action="store_true", help="compare current vs baseline; list breaches (still exits 0 unless --strict)")
    ap.add_argument("--strict", action="store_true", help="with --check, exit 1 if any function exceeds --fail-ratio x baseline")
    ap.add_argument(
        "--fail-ratio",
        type=float,
        default=3.0,
        help="generous ceiling: current/baseline ratio (on avg, falling back to p99/total) that trips --strict (default 3.0)",
    )
    ap.add_argument("--write-baseline", action="store_true", help="write the current run as the new baseline and exit")
    args = ap.parse_args()

    top = load_report(args.input)
    current = extract_current(top)

    if args.write_baseline:
        write_baseline(args.baseline, current)
        print(f"wrote {len(current)} function baseline(s) to {args.baseline}")
        return 0

    baseline_top = load_report(args.baseline)
    baseline = extract_current(baseline_top)

    breaches = compute_breaches(current, baseline, args.fail_ratio) if args.check else []
    write_summary(render_summary(current, baseline, args.fail_ratio, args.check, breaches))

    if args.check:
        strict_note = "" if args.strict else " [ADVISORY — pass --strict to gate]"
        print(
            f"bench-metrics: {len(current)} function(s) parsed, "
            f"{len(breaches)} breach(es) of {args.fail_ratio:g}x baseline{strict_note}"
        )

    if args.strict and breaches:
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())

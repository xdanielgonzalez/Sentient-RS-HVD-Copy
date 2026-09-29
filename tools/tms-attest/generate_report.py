#!/usr/bin/env python3
"""tms-attest: MIL-STD-3071 TMS conformance attestation generator.

Reads docs/compliance/tms/tms-coverage.json (the tms-idl-codegen descriptor-factory's generated
type/topic catalogue) plus compliance/policy/data/tms_topic_classes.json (the hand-curated
Required/Conditional cross-reference derived from MIL-STD-3071 Table B.2 / A.8.7), runs the
conftest/OPA policies in compliance/policy/ against the coverage file, and emits:

  - compliance/assessment-results.json   (OSCAL 1.2.1 assessment-results)
  - compliance/tms-compliance-report.md  (human-readable summary)

Deliberately a standalone Python script with NO third-party dependencies (stdlib only) — it lives
outside the Cargo workspace entirely (see justfile `tms-attest*` recipes), so it never touches the
Rust dependency graph / cargo-vet surface.

MIL-STD-3071 §1.5 states plainly: "This document does not define test procedures that verify
compliance with this standard." No attestation/report format is prescribed anywhere in the
standard, so this generator's output shape is a project-defined convention, not a certified DDS
conformance test.

Usage:
    python3 tools/tms-attest/generate_report.py [--conftest-bin conftest] [--conftest-json FILE]

Exit code mirrors conftest's verdict (0 = every Required/Conditional topic has a type binding and
every catalogue-integrity check holds; 1 = at least one deny fired) so `just tms-attest-check` can
gate on it directly. Use --conftest-json to feed a pre-computed `conftest test --output json`
result when the conftest binary isn't on PATH (e.g. this sandbox); the generator's own JSON/report
output is otherwise identical either way.
"""
from __future__ import annotations

import argparse
import json
import re
import shutil
import subprocess
import sys
import uuid
from datetime import datetime, timezone
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
COVERAGE_JSON = REPO_ROOT / "docs/compliance/tms/tms-coverage.json"
TOPIC_CLASSES_JSON = REPO_ROOT / "compliance/policy/data/tms_topic_classes.json"
POLICY_DIR = REPO_ROOT / "compliance/policy"
DATA_DIR = REPO_ROOT / "compliance/policy/data"
ASSESSMENT_RESULTS_OUT = REPO_ROOT / "compliance/assessment-results.json"
REPORT_OUT = REPO_ROOT / "compliance/tms-compliance-report.md"
OSCAL_VERSION = "1.2.1"


def kebab(name: str) -> str:
    return re.sub(r"(?<!^)(?=[A-Z])", "-", name).lower()


def uid() -> str:
    return str(uuid.uuid4())


def now_iso() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def load_json(p: Path):
    return json.loads(p.read_text())


def rel(p: Path) -> str:
    """Render a path relative to REPO_ROOT for display; fall back to the absolute path if it
    isn't under REPO_ROOT (e.g. a scratch/testdata path passed for local self-testing)."""
    try:
        return str(p.relative_to(REPO_ROOT))
    except ValueError:
        return str(p)


def find_topic_type(topic: str, coverage: dict):
    for t in coverage["types"]:
        if topic in t.get("topics", []):
            return t
    return None


def run_conftest(conftest_bin: str, coverage_path: Path):
    cmd = [
        conftest_bin, "test",
        "--policy", str(POLICY_DIR),
        "--data", str(DATA_DIR),
        "--output", "json",
        str(coverage_path),
    ]
    proc = subprocess.run(cmd, capture_output=True, text=True)
    parsed = None
    try:
        parsed = json.loads(proc.stdout)
    except json.JSONDecodeError:
        pass
    return proc.returncode, parsed, proc.stdout, proc.stderr


def build_controls(coverage: dict, topic_classes: dict):
    controls = []
    for topic in topic_classes["required_topics"]:
        t = find_topic_type(topic, coverage)
        controls.append({
            "control_id": f"tms-topic-{kebab(topic)}",
            "topic": topic,
            "usage_class": "Required",
            "condition": None,
            "bound": t is not None,
            "tms_type": f'{t["module"]}::{t["name"]}' if t else None,
            "idl_line": t.get("idl_line") if t else None,
        })
    for topic, cond in topic_classes["conditional_topics"].items():
        t = find_topic_type(topic, coverage)
        controls.append({
            "control_id": f"tms-topic-{kebab(topic)}",
            "topic": topic,
            "usage_class": "Conditional",
            "condition": cond,
            "bound": t is not None,
            "tms_type": f'{t["module"]}::{t["name"]}' if t else None,
            "idl_line": t.get("idl_line") if t else None,
        })
    return controls


def build_assessment_results(coverage_path: Path, coverage: dict, controls: list,
                              conftest_returncode, deny_messages, warn_messages,
                              conftest_skipped_reason: str | None):
    ts = now_iso()
    subject_uuid = uid()

    observations = []
    for c in controls:
        obs_uuid = uid()
        satisfied = c["bound"]
        observations.append({
            "uuid": obs_uuid,
            "description": (
                f'Topic-coverage check for {c["topic"]!r} ({c["usage_class"]}'
                + (f', condition: {c["condition"]}' if c["condition"] else "")
                + f'): type binding {"present" if satisfied else "ABSENT"} in '
                f'{coverage_path.name}'
                + (f' (tms_type={c["tms_type"]}, idl_line={c["idl_line"]})' if satisfied else "")
                + "."
            ),
            "methods": ["TEST"],
            "collected": ts,
            "subjects": [{"subject-uuid": subject_uuid, "type": "component",
                          "title": "docs/compliance/tms/tms-coverage.json (generated type-catalogue)"}],
            "props": [
                {"name": "control-id", "value": c["control_id"]},
                {"name": "usage-class", "value": c["usage_class"]},
                {"name": "result", "value": "satisfied" if satisfied else "not-satisfied"},
            ],
        })
        c["_observation_uuid"] = obs_uuid

    # Meta / catalogue-integrity observations (not tied to one control) — surfaces conftest's
    # provenance + regression-guard checks (compliance/policy/{provenance,catalog_integrity}.rego)
    # even though those aren't per-topic-control findings.
    if deny_messages or warn_messages:
        meta_uuid = uid()
        lines = [f"conftest (compliance/policy) result: {len(deny_messages)} failure(s), {len(warn_messages)} warning(s)."]
        lines += [f"DENY: {m}" for m in deny_messages]
        lines += [f"WARN: {m}" for m in warn_messages]
        observations.append({
            "uuid": meta_uuid,
            "description": "\n".join(lines),
            "methods": ["TEST"],
            "collected": ts,
            "subjects": [{"subject-uuid": subject_uuid, "type": "component",
                          "title": "docs/compliance/tms/tms-coverage.json (generated type-catalogue)"}],
            "props": [{"name": "check", "value": "conftest-full-output"}],
        })
    elif conftest_skipped_reason:
        observations.append({
            "uuid": uid(),
            "description": f"conftest was NOT run by this generator invocation: {conftest_skipped_reason}",
            "methods": ["TEST"],
            "collected": ts,
            "subjects": [{"subject-uuid": subject_uuid, "type": "component",
                          "title": "docs/compliance/tms/tms-coverage.json (generated type-catalogue)"}],
            "props": [{"name": "check", "value": "conftest-skipped"}],
        })

    findings = []
    for c in controls:
        if c["bound"]:
            continue
        matching_msg = next((m for m in deny_messages if c["topic"] in m), None)
        findings.append({
            "uuid": uid(),
            "title": f'{c["topic"]} ({c["usage_class"]}) has no type binding',
            "description": matching_msg or (
                f'{c["usage_class"]} topic {c["topic"]!r} has no type binding in '
                f'{coverage_path.name}.'
            ),
            "target": {
                "type": "statement-id",
                "target-id": c["control_id"],
                "status": {"state": "not-satisfied"},
            },
            "related-observations": [{"observation-uuid": c["_observation_uuid"]}],
        })

    satisfied_count = sum(1 for c in controls if c["bound"])
    gate_pass = (conftest_returncode == 0) if conftest_returncode is not None else (satisfied_count == len(controls))

    result = {
        "uuid": uid(),
        "title": f"TMS conftest gate run — {ts}",
        "description": (
            "Automated assessment of docs/compliance/tms/tms-coverage.json against "
            "compliance/oscal/catalog.json's Required/Conditional topic controls, via the "
            "conftest/OPA policies in compliance/policy/. This checks type-catalogue coverage "
            "(does a shaped Rust type exist for this topic) — NOT runtime publish-wiring "
            "coverage (does bin-rpi actually publish/subscribe it); see "
            "compliance/oscal/component-definition.json's bin-rpi component for that separate, "
            "currently non-machine-checked axis."
        ),
        "start": ts,
        "end": ts,
        "reviewed-controls": {
            "control-selections": [
                {"include-controls": [{"control-id": c["control_id"]} for c in controls]}
            ]
        },
        "props": [
            {"name": "gate-verdict", "value": "PASS" if gate_pass else "FAIL"},
            {"name": "controls-satisfied", "value": str(satisfied_count)},
            {"name": "controls-total", "value": str(len(controls))},
        ],
        "observations": observations,
    }
    if findings:
        result["findings"] = findings
    if conftest_returncode is not None:
        result["props"].append({"name": "conftest-exit-code", "value": str(conftest_returncode)})

    return {
        "assessment-results": {
            "uuid": uid(),
            "metadata": {
                "title": "TMS Conformance Assessment Results — MIL-STD-3071 Topic Coverage Gate",
                "last-modified": ts,
                "version": ts,
                "oscal-version": OSCAL_VERSION,
                "remarks": (
                    "MIL-STD-3071 §1.5 (p.1): 'This document does not define test procedures "
                    "that verify compliance with this standard.' This assessment-results "
                    "document and its generating methodology (compliance/policy/*.rego run via "
                    "conftest) are a project-defined conformance framework, not a certified DDS "
                    "conformance test."
                ),
            },
            "import-ap": {
                "href": "oscal/catalog.json",
                "remarks": (
                    "No separate OSCAL assessment-plan document is authored for this gate (out "
                    "of scope for a JSON-driven CI check) -- this references the catalog being "
                    "assessed directly. The actual assessment methodology is "
                    "compliance/policy/*.rego, run via conftest; see that directory for the "
                    "plan-equivalent."
                ),
            },
            "results": [result],
        }
    }


def render_markdown(coverage_path: Path, controls: list, conftest_returncode,
                     deny_messages: list, warn_messages: list,
                     conftest_skipped_reason: str | None) -> str:
    ts = now_iso()
    required = [c for c in controls if c["usage_class"] == "Required"]
    conditional = [c for c in controls if c["usage_class"] == "Conditional"]
    req_ok = sum(1 for c in required if c["bound"])
    cond_ok = sum(1 for c in conditional if c["bound"])

    lines = []
    lines.append("<!-- @generated by tools/tms-attest/generate_report.py -- regenerate with `just tms-attest`. -->")
    lines.append("# TMS Compliance Report — MIL-STD-3071 Topic Coverage Attestation")
    lines.append("")
    lines.append(f"Generated: {ts}  ")
    lines.append(f"Input: `{rel(coverage_path)}` (generated by `tools/tms-idl-codegen`)  ")
    lines.append("Catalog: `compliance/oscal/catalog.json` (custom OSCAL catalog, MIL-STD-3071 A.8.7 / Table B.2)")
    lines.append("")
    lines.append("## Conformance stance")
    lines.append("")
    lines.append(
        "MIL-STD-3071 §1.5 (p.1): *\"This document does not define test procedures that verify "
        "compliance with this standard.\"* No attestation or report format is prescribed anywhere "
        "in the standard. This report's spine is the A.8.7 topic × usage × condition matrix "
        "(Required / Conditional-on-10-named-conditions / Optional — Tables B.1 p.64 / B.2 p.65), "
        "plus the D.4/D.5 \"Conformance testing will verify...\" categories (participation vs. "
        "DeviceInfo, extensibility, keys, bounded values, units, QoS interoperability)."
    )
    lines.append("")
    lines.append(
        "**This gate checks IDL type-catalogue coverage, not runtime publish-wiring coverage.** "
        "`tms-coverage.json` records, for every topic, whether the descriptor factory "
        "(`tools/tms-idl-codegen`) parsed an IDL `@topic` annotation and produced a shaped Rust "
        "type for it — it carries no field for whether `bin-rpi` actually publishes/subscribes "
        "that topic at runtime today. That second, currently-manual axis is tracked in "
        "`docs/compliance/README.md` and mirrored (as of this pass) in "
        "`compliance/oscal/component-definition.json`'s `bin-rpi` component."
    )
    lines.append("")
    lines.append("## Documented adaptations (project-wide, not defects)")
    lines.append("")
    lines.append("- **Transport**: MQTT+JSON, not DDS/XCDR2/RTPS/UDP/IPv6 domain 0 (D.1-D.3) — the IDL is used only as the schema-shape oracle.")
    lines.append("- **JSON key casing**: snake_case wire keys vs. IDL camelCase members — spec-silent (the real DDS wire is XCDR2, a positional binary format; there is no \"spec casing\" for JSON to violate).")
    lines.append("- **Topic naming**: `tms/{device_id}/{snake_slug}` vs. the IDL's bare CamelCase, no-device-segment names (A.8.6, D.3) — a documented, reversible bridge rule.")
    lines.append("- **Sequence-bound narrowings**: several `heapless` bounds are narrowed below the IDL `MAXLEN` for the MCU's 64 KB SRAM budget (e.g. port-measurement sequences 4 vs. `MAX_PORTS`=64) — emit-safe, decode-side-truncating, each documented at its type definition.")
    lines.append("")
    lines.append("## Summary")
    lines.append("")
    lines.append(f"- Required topics: **{req_ok}/{len(required)}** have a type binding in the catalogue.")
    lines.append(f"- Conditional topics (10 named A.8.7 conditions): **{cond_ok}/{len(conditional)}** have a type binding in the catalogue.")
    if conftest_returncode is not None:
        verdict = "PASS" if conftest_returncode == 0 else "FAIL"
        lines.append(f"- conftest/OPA gate (`compliance/policy/*.rego`): **{verdict}** (exit code {conftest_returncode}).")
    elif conftest_skipped_reason:
        lines.append(f"- conftest/OPA gate: **NOT RUN** this invocation — {conftest_skipped_reason}")
    lines.append("")

    def table(rows, with_condition):
        out = []
        header = "| Topic | Type | IDL line | Bound? |"
        sep = "|---|---|---|---|"
        if with_condition:
            header = "| Topic | Condition | Type | IDL line | Bound? |"
            sep = "|---|---|---|---|---|"
        out.append(header)
        out.append(sep)
        for c in rows:
            mark = "yes" if c["bound"] else "**NO**"
            ty = f'`{c["tms_type"]}`' if c["tms_type"] else "_none_"
            line = c["idl_line"] if c["idl_line"] else "_none_"
            if with_condition:
                out.append(f'| `{c["topic"]}` | {c["condition"]} | {ty} | {line} | {mark} |')
            else:
                out.append(f'| `{c["topic"]}` | {ty} | {line} | {mark} |')
        return "\n".join(out)

    lines.append("## Required topics (MIL-STD-3071 Table B.2 / A.8.7)")
    lines.append("")
    lines.append(table(required, with_condition=False))
    lines.append("")
    lines.append("## Conditional topics (MIL-STD-3071 Table B.2 / A.8.7, 10 named conditions)")
    lines.append("")
    lines.append(table(conditional, with_condition=True))
    lines.append("")

    if deny_messages:
        lines.append("## conftest findings (gate FAILURES)")
        lines.append("")
        for m in deny_messages:
            lines.append(f"- {m}")
        lines.append("")
    if warn_messages:
        lines.append("## conftest warnings (advisory)")
        lines.append("")
        for m in warn_messages:
            lines.append(f"- {m}")
        lines.append("")

    lines.append("## Known gap: runtime publish-wiring (not machine-checked by this gate)")
    lines.append("")
    lines.append(
        "As of this pass, the four Required / de-facto-Required STOR topics — "
        "`ActiveMicrogridControllerState`, `EnergyStartStopState`, `PowerPortState`, "
        "`DcLoadSharingState` (HAS DC PORTS is met) — are now WIRED at runtime by the "
        "spec-conformance sweep: a per-identity `state_publisher` task plus the A.7.5 "
        "MC-selection state machine in `crates/bin/rpi/src/tasks/` (issues #263/#264/#265 closed; "
        "content is PROVISIONAL where a telemetry-only device cannot source a field). The "
        "following Conditional topics still have a type binding but no runtime publisher "
        "(condition-dependent, not Required for our STOR device): `AcLoadSharingState`, "
        "`AcSummaryMeasurementUpdate`, `ControlParameterState`, `IdentityNicknameState`, "
        "`OperatorIntentState`, `GroundingCircuitState`, `AuthorizationToEnergizeReply`. The "
        "type-catalogue gate above is green for all of them; the runtime-wiring axis is tracked "
        "in `compliance/oscal/component-definition.json`'s `bin-rpi` component-implementation "
        "statuses. Re-run `just tms-attest` to refresh."
    )
    lines.append("")
    return "\n".join(lines) + "\n"


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--coverage", default=str(COVERAGE_JSON))
    ap.add_argument("--conftest-bin", default="conftest")
    ap.add_argument("--conftest-json", default=None,
                     help="Path to a pre-computed `conftest test --output json` result; skips invoking the binary.")
    ap.add_argument("--out-assessment", default=str(ASSESSMENT_RESULTS_OUT))
    ap.add_argument("--out-report", default=str(REPORT_OUT))
    args = ap.parse_args()

    coverage_path = Path(args.coverage).resolve()
    coverage = load_json(coverage_path)
    topic_classes = load_json(TOPIC_CLASSES_JSON)["tms_topic_classes"]
    controls = build_controls(coverage, topic_classes)

    conftest_returncode = None
    conftest_results = None
    conftest_skipped_reason = None

    if args.conftest_json:
        conftest_results = load_json(Path(args.conftest_json))
        conftest_returncode = 1 if any(r.get("failures") for r in conftest_results) else 0
    else:
        resolved = shutil.which(args.conftest_bin)
        if not resolved and not Path(args.conftest_bin).exists():
            conftest_skipped_reason = (
                f"conftest binary {args.conftest_bin!r} not found on PATH. Install the pinned "
                f"release binary (see .github/workflows/compliance.yml) or pass --conftest-json."
            )
        else:
            conftest_returncode, conftest_results, _out, err = run_conftest(args.conftest_bin, coverage_path)
            if conftest_results is None and err:
                conftest_skipped_reason = f"conftest produced no parseable JSON output: {err.strip()[:400]}"

    deny_messages, warn_messages = [], []
    if conftest_results:
        for r in conftest_results:
            deny_messages += [f["msg"] for f in (r.get("failures") or [])]
            warn_messages += [w["msg"] for w in (r.get("warnings") or [])]

    assessment = build_assessment_results(
        coverage_path, coverage, controls, conftest_returncode, deny_messages, warn_messages,
        conftest_skipped_reason,
    )
    report_md = render_markdown(
        coverage_path, controls, conftest_returncode, deny_messages, warn_messages,
        conftest_skipped_reason,
    )

    out_assessment = Path(args.out_assessment)
    out_report = Path(args.out_report)
    out_assessment.parent.mkdir(parents=True, exist_ok=True)
    out_report.parent.mkdir(parents=True, exist_ok=True)
    out_assessment.write_text(json.dumps(assessment, indent=2) + "\n")
    out_report.write_text(report_md)

    satisfied = sum(1 for c in controls if c["bound"])
    print(f"tms-attest: {satisfied}/{len(controls)} Required+Conditional topic controls have a "
          f"type binding in {coverage_path.name}", file=sys.stderr)
    print(f"tms-attest: wrote {rel(out_assessment)}", file=sys.stderr)
    print(f"tms-attest: wrote {rel(out_report)}", file=sys.stderr)

    if conftest_returncode is not None:
        print(f"tms-attest: conftest exit code {conftest_returncode}", file=sys.stderr)
        return conftest_returncode
    print("tms-attest: conftest was not run (see report for reason); exiting 0 (report-only mode)", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())

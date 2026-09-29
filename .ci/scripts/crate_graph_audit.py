#!/usr/bin/env python3
"""Crate-graph audit — the two workspace-architecture rules no maintained tool covers.

Dependency-DIRECTION / layering is enforced by `cargo-deny` `[bans]` + `wrappers` (see deny.toml) —
a maintained tool already in the supply-chain gate, verified to apply to workspace path deps. This
script only does the residue, for which the Rust ecosystem has no off-the-shelf tool (a thin
`cargo metadata` check is the norm):

  SRP / roster  every workspace member must be declared in `.ci/crate-graph.toml` (a new top-level
                crate fails until someone declares its single responsibility — the "no one-off leaf"
                forcing function: is this a domain, or an aspect that belongs as a module?).
  cohesion      every non-bin, non-test library-shaped crate (`lib-*`, `spec-*`, `impl-*`, ...) must
                be reachable from a shipping binary (no orphan islands) — an exclusion-based check
                (skip only `bin-*`/`test-*`) so a new non-`lib-` contract/policy crate (e.g.
                `spec-app`, ADR 0009) is covered too, not just the `lib-` prefix.

`known_debt` crates are exempt from the orphan rule and reported (not failed): the gate blocks NEW
fracturing while the tracked consolidation issues work the current debt down (baseline-ratchet, same
philosophy as `.ci/scripts/typeimpl_audit.py`).

Usage:
  crate_graph_audit.py --check    # enforce; exit 1 on violations (just lint / CI)
  crate_graph_audit.py --dot      # emit Graphviz DOT of the crate-level graph (just depgraph)
  crate_graph_audit.py --mermaid  # emit the runtime data-flow Mermaid stack (just depgraph -> README)
"""
from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
POLICY = REPO / ".ci" / "crate-graph.toml"

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover
    try:
        import tomli as tomllib  # type: ignore
    except ModuleNotFoundError:
        sys.exit("crate_graph_audit: need Python 3.11+ (tomllib) or `pip install tomli`")


def load_policy() -> dict:
    with POLICY.open("rb") as fh:
        return tomllib.load(fh)


def workspace_graph() -> tuple[set[str], dict[str, set[str]], dict[str, str]]:
    """Return (members, normal_adjacency, layer_by_crate). Only workspace<->workspace normal edges."""
    meta = json.loads(subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--no-deps"],
        cwd=REPO, capture_output=True, text=True, check=True,
    ).stdout)
    members = {p["name"] for p in meta["packages"]}
    adj: dict[str, set[str]] = {}
    for p in meta["packages"]:
        adj[p["name"]] = {
            d["name"] for d in p.get("dependencies", [])
            if d["name"] in members and (d.get("kind") or "normal") == "normal"
        }
    return members, adj, {}


def reachable(roots: list[str], adj: dict[str, set[str]]) -> set[str]:
    seen, stack = set(), list(roots)
    while stack:
        for nxt in adj.get(stack.pop(), ()):
            if nxt not in seen:
                seen.add(nxt)
                stack.append(nxt)
    return seen


def audit() -> int:
    policy = load_policy()
    roots = policy["binary_roots"]
    declared = policy.get("crates", {})
    debt = policy.get("known_debt", {})
    known = set(declared) | set(debt)

    members, adj, _ = workspace_graph()
    reach = reachable(roots, adj)
    violations: list[str] = []

    # SRP / roster — every member declared; no stale entries
    for pkg in sorted(members - known):
        violations.append(
            f"[roster] undeclared crate `{pkg}` — add it to .ci/crate-graph.toml with a one-line "
            f"`responsibility` (one reason to change). Domain, or a one-off aspect that belongs as a "
            f"module of an existing crate?"
        )
    for pkg in sorted(known - members):
        violations.append(f"[stale] `{pkg}` is in .ci/crate-graph.toml but not a workspace member — remove it.")

    # cohesion — orphan library-shaped islands (non-bin, non-test) must reach a shipping binary.
    # Exclusion-based (not `startswith("lib-")`) so every library-shaped crate — lib-*, spec-*,
    # impl-* — is covered, not just the `lib-` prefix (ADR 0009: `spec-app` is a non-`lib-`
    # contract crate that must still be reachable, same as any other library crate). "non-test" is
    # the `layer = "test"` taxonomy this file already declares (test-kit/test-e2e/test-acceptance
    # also happen to be `test-`-prefixed, but the prefix was never the real rule — a declared
    # `layer = "test"` crate without that prefix, e.g. bench/HIL-support tooling, is equally exempt).
    for pkg in sorted(members):
        is_test_layer = declared.get(pkg, {}).get("layer") == "test"
        if pkg.startswith("bin-") or is_test_layer or pkg in roots or pkg in reach or pkg in debt:
            continue
        violations.append(
            f"[cohesion] orphan crate `{pkg}` — reachable from none of {roots}. Dead-weight: wire it into "
            f"the shipping path, fold it into a consumer, or delete it."
        )

    for pkg in sorted(debt):
        d = debt[pkg]
        print(f"[debt] `{pkg}` -> {d.get('consolidate_into','?')} (#{d.get('issue','?')}): {d.get('reason','')}")
    if violations:
        print()
        for v in violations:
            print("VIOLATION " + v)
        print(f"\ncrate-graph audit: {len(violations)} violation(s), {len(debt)} tracked debt item(s). "
              f"(Layering is enforced separately by cargo-deny [bans]; see deny.toml.)")
        return 1
    print(f"\ncrate-graph audit OK — {len(members)} crates, roster complete + no new orphan, "
          f"{len(debt)} tracked debt item(s). Layering enforced by cargo-deny [bans].")
    return 0


LAYER_COLORS = {
    "foundation": "#dfefff", "domain": "#e6ffe6", "bridge": "#fff2cc", "contract": "#ccfff2",
    "service": "#ffe6cc", "app": "#f0e0ff", "platform": "#ffe0e0", "bin": "#ffd0d0", "test": "#eeeeee",
}

# --- Runtime data-flow view (the README architecture diagram) --------------------------------------
# How telemetry moves UP from the serial device and OUT via MQTT, plus the runtime that runs the app.
# This is an EDITORIAL grouping — a crate's ROLE in the runtime pipeline — deliberately DISTINCT from
# the compile-time dependency `layer` above that cargo-deny enforces (that view is `--dot`/crates.svg).
# Bands are ordered top (interface/out) -> bottom (device/runtime) and coloured by ZONE.
FLOW_TIERS = [  # (tier id, band label, zone) — top to bottom
    ("interface",    "interface · the MQTT broker the world subscribes to",         "egress"),
    ("transport",    "transport · carry telemetry out over the network",            "egress"),
    ("presentation", "presentation · canonical → TMS wire form",                    "data"),
    ("model",        "model · compact in-memory telemetry",                         "data"),
    ("decode",       "decode · vendor wire → canonical",                            "data"),
    ("serial",       "serial link · RS-485 / RS-232 bytes",                         "data"),
    ("runtime",      "runtime · runs the app, provides the platform, secures it",   "runtime"),
]
FLOW_OF = {
    "lib-mqtt": "interface",
    "lib-net": "transport",
    "lib-mapping": "presentation", "lib-tms": "presentation",
    "lib-canonical": "model", "lib-store": "model",
    "lib-parse": "decode",
    "lib-io": "serial",
    "spec-app": "runtime", "impl-app": "runtime", "bin-rpi-platform": "runtime",
    "bin-mcu-platform": "runtime", "bin-rpi": "runtime", "lib-security": "runtime",
}
ZONE_FILL = {"egress": "#d6e8e2", "data": "#dde7f2", "runtime": "#e6e3ee"}
ZONE_STROKE = {"egress": "#8fb3a8", "data": "#9db0c8", "runtime": "#b0a9c2"}


def emit_dot() -> int:
    policy = load_policy()
    all_decl = {**policy.get("crates", {}), **policy.get("known_debt", {})}
    debt = set(policy.get("known_debt", {}))
    members, adj, _ = workspace_graph()
    print("// GENERATED by .ci/scripts/crate_graph_audit.py --dot (via `just depgraph`). DO NOT EDIT.")
    print("digraph crates {")
    print('  rankdir=TB; node [shape=box, style="filled,rounded", fontname="monospace"];')
    for pkg in sorted(members):
        color = LAYER_COLORS.get(all_decl.get(pkg, {}).get("layer"), "#ffffff")
        label = pkg + ("\\n(debt)" if pkg in debt else "")
        border = ' color="#cc0000" penwidth=2' if pkg in debt else ""
        print(f'  "{pkg}" [label="{label}" fillcolor="{color}"{border}];')
    for pkg in sorted(members):
        for dep in sorted(adj.get(pkg, ())):
            print(f'  "{pkg}" -> "{dep}";')
    print("}")
    return 0


def emit_mermaid() -> int:
    """Emit the runtime DATA-FLOW stack (the README architecture diagram): telemetry flows UP from the
    serial device through decode/model/mapping and OUT via the MQTT interface + network; a runtime
    base runs the app. Bands are grouped by role into three colour ZONES (egress / data / runtime).
    Editorial view — distinct from the compile-time dependency `layer` cargo-deny enforces
    (`--dot`/crates.svg). Crates absent from FLOW_OF (e.g. test crates) are omitted."""
    members, _adj, _ = workspace_graph()
    missing = sorted(c for c in members if c not in FLOW_OF and not c.startswith("test-"))
    if missing:
        print(f"%% NOTE: crates absent from the flow view (add to FLOW_OF): {missing}", file=sys.stderr)
    by_tier: dict[str, list[str]] = {}
    for c in members:
        if c in FLOW_OF:
            by_tier.setdefault(FLOW_OF[c], []).append(c)

    def cid(c: str) -> str:
        return "c_" + c.replace("-", "_")

    print("%% GENERATED by .ci/scripts/crate_graph_audit.py --mermaid (via `just depgraph`). DO NOT EDIT.")
    print("%% Runtime data-flow view: telemetry flows up from the device and out via MQTT.")
    print("flowchart TB")
    ids: list[str] = []
    chain: list[str] = []
    for tier, label, _zone in FLOW_TIERS:
        if tier not in by_tier:
            continue
        crates = sorted(by_tier[tier])
        print(f'    subgraph L_{tier}["{label}"]')
        print("        direction LR")
        # `~~~` chains the crates left-to-right (Mermaid ignores a subgraph's `direction LR` once it
        # is externally linked by the vertical band stack, so without this they stack vertically).
        print("        " + " ~~~ ".join(f'{cid(c)}["{c}"]' for c in crates))
        ids.extend(cid(c) for c in crates)
        print("    end")
        chain.append(f"L_{tier}")
        if tier == "serial":  # the physical device feeds the serial link, one row below it
            print('    device["physical device · Seplos BMS · Voltronic inverter"]')
            chain.append("device")
    print("    " + " ~~~ ".join(chain))
    # flow direction: telemetry rises from the device into the serial link; leaves via the interface.
    print("    device -.->|telemetry up| c_lib_io")
    print("    c_lib_mqtt -.->|out| out((subscribers · cloud))")
    print("    classDef crate fill:#ffffff,stroke:#b6bcc4,color:#1a1a1a;")
    print("    class " + ",".join(ids) + " crate")
    print("    classDef hw fill:#f4f4f6,stroke:#b0b0b0,color:#333,stroke-dasharray:4 3;")
    print("    class device,out hw")
    for tier, _label, zone in FLOW_TIERS:
        if tier in by_tier:
            print(f"    style L_{tier} fill:{ZONE_FILL[zone]},stroke:{ZONE_STROKE[zone]},color:#1a1a1a")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--check", action="store_true", help="enforce roster + orphan; exit 1 on violations")
    ap.add_argument("--dot", action="store_true", help="emit Graphviz DOT of the crate-level graph")
    ap.add_argument("--mermaid", action="store_true", help="emit the runtime data-flow Mermaid stack")
    args = ap.parse_args()
    if args.dot:
        return emit_dot()
    if args.mermaid:
        return emit_mermaid()
    return audit()


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3
"""Security-tier matrix drift audit — `justfile`'s `sec-matrix` recipe vs
`.github/workflows/_security.yml`'s `TIERS` bash array.

Commit af2d06a collapsed the 9-job security matrix in `_security.yml` into a single job
that loops a bash array of feature tiers. The SAME nine tiers are ALSO declared in
`justfile`'s `sec-matrix` recipe (as a chain of `(sec-tier "...")` dependencies, since
`just`'s dependency mechanism can't give the workflow the all-failures-visible reporting
it deliberately preserves — see `_security.yml`'s own step comment). Nothing enforces
these two lists staying in sync: adding a tier to one and not the other silently reduces
CI coverage while everything else stays green. This script is that enforcement.

Compares the two tier lists as ORDERED sequences (not just sets) — a same-set-but-
reordered pair is functionally harmless but is a real signal that one file was edited and
the other wasn't, and it's free to catch.

Usage:
  sec_matrix_audit.py --check    # gate: exit 1 on any drift (just lint / CI)
  sec_matrix_audit.py            # human diff view, always exits 0
"""
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
JUSTFILE = REPO / "justfile"
WORKFLOW = REPO / ".github/workflows/_security.yml"

SEC_MATRIX_LINE_RE = re.compile(r"^sec-matrix\s*:(.*)$", re.MULTILINE)
SEC_TIER_CALL_RE = re.compile(r'\(sec-tier\s+"([^"]*)"\)')
TIERS_BLOCK_RE = re.compile(r"declare -a TIERS=\((.*?)\n\s*\)", re.DOTALL)
QUOTED_RE = re.compile(r'"([^"]*)"')

MIN_EXPECTED_TIERS = 3  # sanity floor — a real parse returns the full 9, never 0/1


def parse_justfile(path: Path) -> list[str]:
    """Extract the ordered tier list from `sec-matrix`'s dependency chain: each
    `(sec-tier "...")` call, in order, plus a trailing empty-string entry IF the bare
    `sec-no-std` dependency (the no_std-core tier, which has no `sec-tier` wrapper) is
    present — that bare dependency is what corresponds to the workflow's `""` entry.
    """
    if not path.exists():
        sys.exit(f"sec_matrix_audit: justfile not found at {path}")
    text = path.read_text()
    m = SEC_MATRIX_LINE_RE.search(text)
    if not m:
        sys.exit(
            f"sec_matrix_audit: could not find a 'sec-matrix:' recipe line in {path} "
            "— file structure changed; fix the parser, don't ignore this."
        )
    line = m.group(1)
    tiers = SEC_TIER_CALL_RE.findall(line)
    if not re.search(r"\bsec-no-std\b", line):
        sys.exit(
            f"sec_matrix_audit: 'sec-matrix' recipe in {path} has no bare 'sec-no-std' "
            "dependency — expected structure changed, cannot parse safely."
        )
    tiers.append("")  # sec-no-std <-> the workflow's "" (no_std core) entry
    if len(tiers) < MIN_EXPECTED_TIERS:
        sys.exit(
            f"sec_matrix_audit: parsed only {len(tiers)} tier(s) from {path}'s "
            "'sec-matrix' recipe — suspiciously few; parser is likely broken, not the recipe."
        )
    return tiers


def parse_workflow(path: Path) -> list[str]:
    """Extract the ordered tier list from the `declare -a TIERS=(...)` bash array,
    stripping trailing `# comment` on each entry before pulling the quoted strings out.
    """
    if not path.exists():
        sys.exit(f"sec_matrix_audit: workflow file not found at {path}")
    text = path.read_text()
    m = TIERS_BLOCK_RE.search(text)
    if not m:
        sys.exit(
            f"sec_matrix_audit: could not find a 'declare -a TIERS=(...)' block in {path} "
            "— file structure changed; fix the parser, don't ignore this."
        )
    body = m.group(1)
    code_only = "\n".join(line.split("#", 1)[0] for line in body.splitlines())
    tiers = QUOTED_RE.findall(code_only)
    if len(tiers) < MIN_EXPECTED_TIERS:
        sys.exit(
            f"sec_matrix_audit: parsed only {len(tiers)} tier(s) from {path}'s "
            "'TIERS' array — suspiciously few; parser is likely broken, not the workflow."
        )
    return tiers


def diff_report(just_tiers: list[str], wf_tiers: list[str]) -> list[str]:
    lines = []
    just_set, wf_set = set(just_tiers), set(wf_tiers)
    only_just = [t for t in just_tiers if t not in wf_set]
    only_wf = [t for t in wf_tiers if t not in just_set]
    if only_just:
        lines.append(f"  in {JUSTFILE} sec-matrix but NOT in {WORKFLOW} TIERS: {only_just}")
    if only_wf:
        lines.append(f"  in {WORKFLOW} TIERS but NOT in {JUSTFILE} sec-matrix: {only_wf}")
    if not only_just and not only_wf and just_tiers != wf_tiers:
        # same set, different order
        lines.append(f"  same tiers, but ORDER differs:")
        lines.append(f"    {JUSTFILE} sec-matrix order: {just_tiers}")
        lines.append(f"    {WORKFLOW} TIERS order:      {wf_tiers}")
    if len(just_tiers) != len(wf_tiers):
        lines.append(
            f"  count mismatch: {len(just_tiers)} tier(s) in {JUSTFILE}, "
            f"{len(wf_tiers)} tier(s) in {WORKFLOW}"
        )
    return lines


def main() -> int:
    args = sys.argv[1:]
    just_tiers = parse_justfile(JUSTFILE)
    wf_tiers = parse_workflow(WORKFLOW)

    if just_tiers == wf_tiers:
        if "--check" in args:
            print(
                f"sec-matrix audit OK — {len(just_tiers)} tiers match, in order, between "
                f"{JUSTFILE} and {WORKFLOW}."
            )
        else:
            print(f"OK — {len(just_tiers)} tiers, identical order:")
            for t in just_tiers:
                print(f"  {t or '(no_std core)'}")
        return 0

    report = diff_report(just_tiers, wf_tiers)
    print(
        f"VIOLATION [sec-matrix drift] {JUSTFILE}'s 'sec-matrix' recipe and "
        f"{WORKFLOW}'s 'TIERS' array have diverged:"
    )
    for line in report:
        print(line)
    print(
        "\nFix: make the two tier lists match exactly, in the same order — add/remove/reorder "
        f"'(sec-tier \"...\")' entries in {JUSTFILE}'s sec-matrix recipe and/or the TIERS=(...) "
        f"array in {WORKFLOW} until they agree."
    )
    return 1


if __name__ == "__main__":
    sys.exit(main())

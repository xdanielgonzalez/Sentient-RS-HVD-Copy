#!/usr/bin/env python3
"""Render a Robot Framework `output.xml` file as a markdown pass/fail table.

Used by CI (_yocto.yml's `e2e` job) to write a per-test-case summary into $GITHUB_STEP_SUMMARY.
Kept as a standalone script (rather than an inline YAML
heredoc) so the workflow's `run: |` block stays simple to indent/maintain and this parser is
independently runnable/testable.

Usage: robot_output_to_summary.py <path-to-output.xml>
"""

import sys
import xml.etree.ElementTree as ET


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: robot_output_to_summary.py <output.xml>", file=sys.stderr)
        return 2

    tree = ET.parse(sys.argv[1])
    print("| test | status | message |")
    print("|---|---|---|")
    for test in tree.getroot().iter("test"):
        status = test.find("status")
        st = status.get("status", "?") if status is not None else "?"
        msg = (status.text or "").replace("|", "\\|").replace("\n", " ") if status is not None else ""
        print(f"| `{test.get('name', '')}` | {st} | {msg[:200]} |")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

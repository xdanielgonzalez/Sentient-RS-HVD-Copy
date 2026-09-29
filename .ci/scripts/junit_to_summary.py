#!/usr/bin/env python3
"""Render a JUnit XML file (e.g. cargo-nextest's junit.xml) as a markdown pass/fail table.

Used by CI (_test.yml's `e2e` job) to write a per-test-case summary into $GITHUB_STEP_SUMMARY.
Kept as a standalone script (rather than an inline YAML heredoc) so the workflow's `run: |` block
stays simple to indent/maintain and this parser is independently runnable/testable.

Usage: junit_to_summary.py <path-to-junit.xml>
"""

import sys
import xml.etree.ElementTree as ET


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: junit_to_summary.py <junit.xml>", file=sys.stderr)
        return 2

    tree = ET.parse(sys.argv[1])
    root = tree.getroot()
    suites = root.findall(".//testsuite") if root.tag != "testsuite" else [root]

    print("| test | status | time (s) |")
    print("|---|---|---|")
    for suite in suites:
        for tc in suite.findall("testcase"):
            name = f"{tc.get('classname', '')}::{tc.get('name', '')}"
            time = tc.get("time", "0")
            if tc.find("failure") is not None or tc.find("error") is not None:
                status = "FAIL"
            elif tc.find("skipped") is not None:
                status = "SKIP"
            else:
                status = "pass"
            print(f"| `{name}` | {status} | {time} |")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3
# .ci/scripts/bitbake-log-filter.py — collapse BitBake's per-task console flood to a progress
# heartbeat, so the Yocto/BitBake CI lanes report STATUS/PROGRESS instead of every task.
#
# WHY: BitBake's `knotty` UI, with no TTY (which is every CI run), can't render its live progress
# bar, so it falls back to printing ONE plain line per task transition:
#     NOTE: Running setscene task 888 of 2691 (.../glibc-locale_2.43.bb:do_package_qa_setscene)
#     NOTE: recipe btrfs-tools-6.19.1-r0: task do_populate_lic_setscene: Succeeded
#     NOTE: recipe libgcc-15.3.0-r0:      task do_package_write_rpm_setscene: Started
# On a warm sstate build that is 5k–15k lines of pure noise per MACHINE, dwarfing the actual
# signal. This filter reads BitBake's stdout on a pipe and:
#   * DROPS the per-recipe task transitions (`recipe X: task Y: Started/Succeeded`) — zero signal.
#   * THROTTLES the `Running (setscene )task N of M` counter to a heartbeat: the first + last task
#     of each phase, phase boundaries, and at most one line every HEARTBEAT_SECS in between — so
#     you still WATCH progress without the flood.
#   * PASSES EVERYTHING ELSE THROUGH UNCHANGED — every WARNING/ERROR, the Sstate summary, the final
#     `Tasks Summary`, parse/cache progress, and any line it does not recognise as noise. A failing
#     build stays fully diagnosable (BitBake's ERROR lines + the failing-task logfile path survive).
#
# It is a PURE stdout filter: it never changes exit status. `set -o pipefail` at the call site
# propagates BitBake's real exit code (the filter is the right-hand side of `bitbake ... | filter`).
#
# Tunables (env):
#   BB_FILTER_HEARTBEAT_SECS  seconds between throttled progress lines (default 15; 0 = keep them all)
#   BB_FILTER_DISABLE         if set to a non-empty value, act as a transparent `cat` (escape hatch)
#
# Host-testable per the repo's "test on the host first" rule: `bitbake-log-filter.py --selftest`
# runs an in-process check that the flood collapses and that warnings/errors survive. No BitBake,
# no emulator needed.

import os
import re
import sys
import time

# Per-recipe task transitions — the bulk of the flood, and pure noise. Tolerates a leading ANSI
# colour escape and an optional timestamp prefix.
RE_RECIPE_TASK = re.compile(
    r"(?:\x1b\[[0-9;]*m)?NOTE: recipe \S.*?: task \S.*?: (?:Started|Succeeded|Running)\b"
)
# The progress counter we throttle down to a heartbeat. BitBake prints one of these per task in
# every phase — "Running setscene task N of M", "Running noexec task N of M" (do_build/do_fetch
# stubs), and the plain "Running task N of M" (main). The optional middle word names the phase, so a
# phase change (setscene -> noexec -> main) is always worth a heartbeat line.
RE_TASK_PROGRESS = re.compile(r"Running (?:(\w+) )?task (\d+) of (\d+)")


def _emit(line):
    try:
        sys.stdout.write(line)
        sys.stdout.flush()
    except BrokenPipeError:
        # Downstream closed (e.g. `| head`); stop quietly rather than dumping a traceback.
        try:
            sys.stdout.close()
        finally:
            os._exit(0)


def run(inp, heartbeat_secs, emit=_emit, now=time.monotonic):
    """Filter `inp` (an iterable of lines) to `emit`. Returns (kept, dropped) counts.

    `emit`/`now` are injectable so the self-test can capture output and drive a fake clock."""
    last_emit = 0.0          # monotonic time of the last progress line we let through
    last_phase = None        # "setscene" | "main" — a phase change is always worth a line
    kept = dropped = 0
    for line in inp:
        # Per-recipe task chatter → drop outright.
        if RE_RECIPE_TASK.search(line):
            dropped += 1
            continue
        m = RE_TASK_PROGRESS.search(line)
        if m:
            phase = m.group(1) or "main"
            n, total = int(m.group(2)), int(m.group(3))
            t = now()
            # Keep on: a phase change, the first/last task of the phase, or once every heartbeat.
            keep = (
                phase != last_phase
                or n <= 1
                or n >= total
                or heartbeat_secs <= 0
                or (t - last_emit) >= heartbeat_secs
            )
            last_phase = phase
            if keep:
                last_emit = t
                emit(line)
                kept += 1
            else:
                dropped += 1
            continue
        # Everything else (warnings, errors, Sstate summary, Tasks Summary, parse progress, …) → keep.
        emit(line)
        kept += 1
    return kept, dropped


def _selftest():
    # A synthetic slice of a real warm-build log: 6 flood lines that MUST collapse, plus the
    # signal lines (warning, error, sstate + tasks summary) that MUST all survive.
    sample = [
        "NOTE: Running setscene task 1 of 4 (a.bb:do_x_setscene)\n",
        "NOTE: recipe btrfs-tools-6.19.1-r0: task do_populate_lic_setscene: Succeeded\n",
        "NOTE: Running setscene task 2 of 4 (b.bb:do_x_setscene)\n",
        "NOTE: recipe libgcc-15.3.0-r0: task do_package_write_rpm_setscene: Started\n",
        "NOTE: Running setscene task 3 of 4 (c.bb:do_x_setscene)\n",
        "NOTE: Running setscene task 4 of 4 (d.bb:do_x_setscene)\n",
        # A phase change (setscene -> noexec) must always print its boundary line, even mid-window.
        "NOTE: Running noexec task 5 of 9 (sentient-image.bb:do_fetch)\n",
        "NOTE: Running task 6 of 9 (sentient-image.bb:do_prepare_recipe_sysroot)\n",
        "WARNING: something worth reading\n",
        "ERROR: recipe foo: task do_compile: Failed\n",
        "ERROR: Logfile of failure stored in: /path/to/log.do_compile.123\n",
        "Sstate summary: Wanted 100 Local 0 Mirrors 98 Missed 2 Current 0 (98% match)\n",
        "NOTE: Tasks Summary: Attempted 5072 tasks of which 5058 didn't need to be rerun and all succeeded.\n",
    ]
    out = []
    # A fixed fake clock (time never advances) means the heartbeat window never elapses, so ONLY the
    # structural keeps (phase change, first/last task) get through — the deterministic worst case.
    kept, dropped = run(sample, heartbeat_secs=15, emit=out.append, now=lambda: 100.0)
    text = "".join(out)
    checks = {
        "every recipe:task line dropped": "recipe btrfs-tools" not in text
        and "libgcc-15.3.0" not in text,
        "phase first task kept": "task 1 of 4" in text,
        "phase last task kept": "task 4 of 4" in text,
        "middle task throttled": "task 2 of 4" not in text and "task 3 of 4" not in text,
        "noexec phase boundary kept": "noexec task 5 of 9" in text,
        "main phase boundary kept": "Running task 6 of 9" in text,
        "WARNING survives": "WARNING: something worth reading" in text,
        "ERROR (task failed) survives": "task do_compile: Failed" in text,
        "ERROR (logfile path) survives": "Logfile of failure stored in" in text,
        "Sstate summary survives": "Sstate summary:" in text,
        "Tasks Summary survives": "Tasks Summary:" in text,
    }
    ok = all(checks.values())
    for name, passed in checks.items():
        print(f"  [{'PASS' if passed else 'FAIL'}] {name}", file=sys.stderr)
    print(f"  kept={kept} dropped={dropped}", file=sys.stderr)
    print("SELFTEST: " + ("OK" if ok else "FAILED"), file=sys.stderr)
    return 0 if ok else 1


def main(argv):
    if "--selftest" in argv:
        return _selftest()
    # Read a line at a time via readline (NOT `for line in sys.stdin`, whose read-ahead buffering
    # could park a few lines unshown during a long quiet compile and make the log look stalled).
    stdin_lines = iter(sys.stdin.readline, "")
    if os.environ.get("BB_FILTER_DISABLE"):
        for line in stdin_lines:
            _emit(line)
        return 0
    try:
        heartbeat = float(os.environ.get("BB_FILTER_HEARTBEAT_SECS", "15"))
    except ValueError:
        heartbeat = 15.0
    run(stdin_lines, heartbeat_secs=heartbeat)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

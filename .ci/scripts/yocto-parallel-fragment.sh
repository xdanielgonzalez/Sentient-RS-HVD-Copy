#!/usr/bin/env bash
# .ci/scripts/yocto-parallel-fragment.sh — compute cgroup-aware, MEMORY-CAPPED BitBake parallelism
# and write a kas local_conf_header fragment setting BB_NUMBER_THREADS/PARALLEL_MAKE.
#
# Extracted from _yocto.yml's "Build sentient-image (kas)" step (the source of truth — it is
# memory-capped; an older CPU-only copy in production.yml's `sysext` job was a latent OOM risk,
# fixed by rewiring it to call this script instead). Behavior-preserving port: the CPU-count read
# (cgroup v2 cpu.max, falling back to nproc), the memory-capped J = min(N, MEMGB/4) formula (RAM
# read from cgroup v2 memory.max, falling back to /proc/meminfo), the emitted fragment shape, and
# the `::notice::` line are all unchanged from the _yocto.yml original.
#
# Usage:
#   .ci/scripts/yocto-parallel-fragment.sh <output-fragment-path>
#   OUT_FRAGMENT=<path> .ci/scripts/yocto-parallel-fragment.sh
#
# Writes the kas limits fragment to $1 (or $OUT_FRAGMENT if $1 is unset) and echoes the
# `::notice::Yocto build limits: ...` line. Callers interpolate the fragment path into their
# `KASCFG="<config>:<fragment>"` kas invocation exactly as before.
set -euo pipefail

OUT="${1:-${OUT_FRAGMENT:?usage: yocto-parallel-fragment.sh <output-fragment-path> (or set OUT_FRAGMENT)}}"

# CPU count: BitBake's default (sched_getaffinity) is blind to a cgroup CFS quota — read the
# real limit from cgroup v2 cpu.max, fall back to nproc.
N=""
if [ -r /sys/fs/cgroup/cpu.max ]; then
  read -r q p < /sys/fs/cgroup/cpu.max || true
  [ "${q:-max}" != "max" ] && [ -n "${p:-}" ] && N=$(( (q + p - 1) / p ))
fi
N="${N:-$(nproc)}"; { [ "$N" -ge 1 ]; } 2>/dev/null || N=4

# MEMORY BOUND (no icecc -> all compiles run locally): BB_NUMBER_THREADS x PARALLEL_MAKE
# concurrent jobs OOM-kill the pod (exit 137, seen at 16x16 on this ARC pool). Cap the thread
# count by RAM (~4 GB per concurrent recipe) from cgroup v2 memory.max (else /proc/meminfo);
# BitBake's PSI-based BB_PRESSURE_MAX_MEMORY (manifest) additionally throttles task launch.
MEMMAX=$(cat /sys/fs/cgroup/memory.max 2>/dev/null || echo max)
if [ "$MEMMAX" = "max" ] || ! [ "$MEMMAX" -eq "$MEMMAX" ] 2>/dev/null; then
  MEMGB=$(( $(awk '/MemTotal/{print $2}' /proc/meminfo 2>/dev/null || echo 0) / 1024 / 1024 ))
else
  MEMGB=$(( MEMMAX / 1024 / 1024 / 1024 ))
fi

# MEMORY-CAPPED parallelism (reverts the earlier "MAX PROCS" directive). A cold build compiles
# ~2400 recipes, and a burst of heavy do_compile OOM-kills the pod (exit 137, seen at ~95 min)
# even with BB_PRESSURE_MAX_MEMORY throttling task launch. Cap BB_NUMBER_THREADS/PARALLEL_MAKE by
# RAM so a cold build SURVIVES to completion and seeds the mirror; never exceed the CPU count.
#
# The divisor was ~4 GB per concurrent recipe, which starved these boxes: on the 22 GB / 14-core
# ARC nodes that yields J = min(14, 5) = 5, i.e. 5 threads at -j5 on 14 cores, and the runner
# measurably idled (worker at 0.1% CPU / 2 GB during a build). Halved to ~2 GB per concurrent
# recipe -> J = min(14, 11) = 11, which actually uses the CPUs we have.
#
# Why this is safe NOW when it wasn't before: the original 4 GB figure was justified by "while the
# MinIO write-through is down, an OOM preserves NOTHING, so it never converges." That premise no
# longer holds — the sstate mirror is back (its PV had been pointing at the dead NFS host
# 10.0.0.246 for 5+ days), so `mc mirror --watch` streams every object out as it is produced and an
# OOM now costs only the in-flight tasks, not the whole build. BB_PRESSURE_MAX_MEMORY (manifest,
# raised to 20000) remains the adaptive backstop. If a cold build does start OOMing (exit 137),
# raise this divisor back toward 3-4 rather than reverting the pressure threshold.
JMEM=$(( MEMGB / 2 )); { [ "$JMEM" -ge 1 ]; } 2>/dev/null || JMEM=1
J="$N"; [ "$JMEM" -lt "$J" ] && J="$JMEM"
echo "::notice::Yocto build limits: cpu=$N RAM=${MEMGB}GB mem-cap=$JMEM -> BB_NUMBER_THREADS=PARALLEL_MAKE=$J (product $(( J * J )))"
{ printf 'header:\n  version: 17\nlocal_conf_header:\n  limits: |\n'; \
  printf '    BB_NUMBER_THREADS = "%s"\n    PARALLEL_MAKE = "-j %s"\n' "$J" "$J"; } > "$OUT"

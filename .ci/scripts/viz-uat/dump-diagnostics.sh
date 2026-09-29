#!/usr/bin/env bash
# .ci/scripts/viz-uat/dump-diagnostics.sh — on any assert failure, capture the cluster-side
# ground truth CI otherwise discards: pod states, recent events, and app/sim/telegraf logs
# (current + previous, so crashloops show their dying words). Every command is best-effort:
# this script must never mask the real failure, only explain it. (Added after run #103, where
# "no fresh battery_cell rows" left nothing to diagnose with.)
#
# Env contract: NS, RELEASE, RUNNER_TEMP (kubeconfig + kubectl live there).
set -uo pipefail

if [ ! -f "$RUNNER_TEMP/kubeconfig" ]; then
  echo "no kubeconfig written — install step failed before cluster contact"
  exit 0
fi
export KUBECONFIG="$RUNNER_TEMP/kubeconfig"
KUBECTL="$RUNNER_TEMP/kubectl"
echo "::group::pods"
"$KUBECTL" -n "$NS" get pods -o wide || true
echo "::endgroup::"
echo "::group::events (most recent 40)"
"$KUBECTL" -n "$NS" get events --sort-by=.lastTimestamp 2>/dev/null | tail -40 || true
echo "::endgroup::"
for d in app sim telegraf; do
  echo "::group::logs deploy/$RELEASE-$d (current, tail 120)"
  "$KUBECTL" -n "$NS" logs "deploy/$RELEASE-$d" --all-containers --tail=120 || true
  echo "::endgroup::"
  echo "::group::logs deploy/$RELEASE-$d (previous instance, tail 60)"
  "$KUBECTL" -n "$NS" logs "deploy/$RELEASE-$d" --all-containers --previous --tail=60 2>/dev/null || echo "(no previous instance)"
  echo "::endgroup::"
done

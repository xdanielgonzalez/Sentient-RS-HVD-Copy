#!/bin/sh
# Two loops fix two limitations of `seplos-tcp-sim --listen` for long-running use:
#   1. it serves ONE connection and exits when the client disconnects
#      (impl_core_connection.rs: read Ok(0) -> return) — the outer loop re-listens;
#   2. runtime control (behavior/faults/state) is stdin-only — a FIFO feeds stdin, so
#        kubectl exec <pod> -- sh -c 'echo "faults high_voltage_protection" > /ctl/sim.ctl'
#      injects faults without attaching. The cat-loop keeps the pipe open across
#      writers, and each restarted sim inherits the same stdin pipe.
# NOTE: the sim rebuilds its state fresh per connection (sample_14_cell), so
# injected behavior/faults/state RESET whenever the client reconnects (e.g. an
# app-pod restart) — re-inject after a reconnect.
set -u
SIM_PORT="${SIM_PORT:-5001}"
SIM_PACE="${SIM_PACE:-19200}"
# Time-modulated telemetry drift (issue #226): "on"/"off"/a key=value spec (see
# impl_drift_spec.rs). Empty/unset = the sim binary's own default (off, static replay).
SIM_DRIFT="${SIM_DRIFT:-}"
CTL=/ctl/sim.ctl

mkdir -p /ctl
[ -p "$CTL" ] || mkfifo -m 0666 "$CTL"
echo "sim control: echo '<line>' > $CTL   (behavior ... | faults ... | state ...)" >&2

( while :; do [ -p "$CTL" ] || { mkfifo -m 0666 "$CTL" 2>/dev/null || sleep 1; }; cat "$CTL"; done ) | while :; do
  set -- --listen "$SIM_PORT"
  [ "$SIM_PACE" != "0" ] && set -- "$@" --pace "$SIM_PACE"
  # --turnaround-ms requires --pace > 0 (impl_cli.rs rejects it otherwise)
  [ "$SIM_PACE" != "0" ] && [ -n "${SIM_TURNAROUND_MS:-}" ] && set -- "$@" --turnaround-ms "$SIM_TURNAROUND_MS"
  [ -n "$SIM_DRIFT" ] && set -- "$@" --drift "$SIM_DRIFT"
  /usr/local/bin/seplos-tcp-sim "$@"
  echo "seplos-tcp-sim exited rc=$? — re-listening in 1s" >&2
  sleep 1
done

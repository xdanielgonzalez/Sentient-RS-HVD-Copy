#!/usr/bin/env bash
# .ci/scripts/viz-uat/deploy-and-assert.sh — helm-upgrade the viz stack into the standing
# viz-uat namespace and assert real telemetry end-to-end (fresh rows + fault injection).
#
# Env contract (set by the calling workflow step):
#   VIZ_UAT_KUBECONFIG — deployer kubeconfig (empty = images pushed, install skipped)
#   NS, RELEASE        — fixed namespace + release name (idempotent upgrades)
#   REGISTRY, HARBOR_NS — image location
#   IMAGE_TAG          — the uat-<run_id> tag both images were pushed with
#   RUNNER_TEMP        — where setup-k8s-tools installed kubectl/helm
#   GITHUB_STEP_SUMMARY — for the green summary line
set -euo pipefail

if [ -z "$VIZ_UAT_KUBECONFIG" ]; then
  echo "::notice::VIZ_UAT_KUBECONFIG not set — images pushed, install skipped (bring-up: .ci/k8s/viz-uat/README.md)"
  exit 0
fi
install -m 600 /dev/null "$RUNNER_TEMP/kubeconfig"
printf '%s' "$VIZ_UAT_KUBECONFIG" > "$RUNNER_TEMP/kubeconfig"
export KUBECONFIG="$RUNNER_TEMP/kubeconfig"
HELM="$RUNNER_TEMP/helm"; KUBECTL="$RUNNER_TEMP/kubectl"

# Namespace pre-exists (bring-up); the SA cannot create namespaces by design.
# Persistence stays ON: this is a standing environment — the PVC carries the full
# telemetry history across upgrades, which is the point of the Grafana demo.
#
# Tailnet exposure auto-enables once bring-up has provisioned the auth key (or the
# node state already exists from a prior join) — pre-bring-up merges keep deploying
# without the sidecar, so `helm --wait` never blocks on an unjoinable tailscale
# container. rbac.create=false: the deployer SA cannot create RBAC objects; the
# bring-up manifest carries the state-Secret Role/RoleBinding (.ci/k8s/viz-uat/rbac.yaml).
TS_FLAGS=""
if "$KUBECTL" -n "$NS" get secret "$RELEASE-tailscale-auth" >/dev/null 2>&1 \
   || "$KUBECTL" -n "$NS" get secret "$RELEASE-tailscale-state" >/dev/null 2>&1; then
  TS_FLAGS="--set tailscale.enabled=true --set tailscale.rbac.create=false"
  echo "tailnet exposure ON (auth/state secret present) — http://sentient-viz.<tailnet>.ts.net"
else
  echo "::notice::tailnet exposure off — create secret $RELEASE-tailscale-auth in $NS to enable (.ci/k8s/viz-uat/README.md)"
fi
# TS_FLAGS deliberately unquoted: it is a flag list (empty or two --set pairs).
# shellcheck disable=SC2086
"$HELM" upgrade --install "$RELEASE" deploy/charts/sentient-viz -n "$NS" \
  --set images.app.repository="$REGISTRY/$HARBOR_NS/sentient-viz-app" \
  --set images.app.tag="$IMAGE_TAG" \
  --set images.sim.repository="$REGISTRY/$HARBOR_NS/sentient-viz-sim" \
  --set images.sim.tag="$IMAGE_TAG" \
  $TS_FLAGS \
  --wait --timeout 5m

# The runner pod is in-cluster: hit services via cluster DNS, no port-forward.
INFLUX="http://$RELEASE-influxdb.$NS.svc.cluster.local:8086"
GRAFANA="http://$RELEASE-grafana.$NS.svc.cluster.local:3000"
TOKEN=sentient-demo-token
flux() { curl -sf "$INFLUX/api/v2/query?org=sentient" \
           -H "Authorization: Token $TOKEN" -H 'Content-Type: application/vnd.flux' \
           -H 'Accept: application/csv' -d "$1"; }

# Steady-state gates: FRESH rows, not measurement existence — the standing PVC keeps
# history from prior runs, so a schema-level check passes vacuously forever.
#
# Gate 1 — app liveness. A fresh heartbeat proves app pod + MQTT + telegraf + InfluxDB.
# It does NOT prove the serial link: the app's 1 s heartbeat ticker runs independently
# of serial health BY DESIGN (degraded start — the adapter keeps heartbeating and
# petting the watchdog while the link is down and it retries). Run #103 failed exactly
# this way: heartbeat fresh, battery_cell never arrived.
echo "gate 1: app liveness (fresh heartbeat)…"
for i in $(seq 18); do
  flux 'from(bucket:"telemetry") |> range(start:-15s) |> filter(fn:(r)=>r._measurement=="heartbeat") |> limit(n:1)' | grep -q heartbeat && break
  [ "$i" -eq 18 ] && { echo "::error::no fresh heartbeat after 90s — app/MQTT/ingest path down (see diagnostics step)"; exit 1; }
  sleep 5
done
# Gate 2 — serial link. The FIRST telemetry measurement is the real serial-link gate:
# rows here require actual Seplos frames decoded over the socat<->sim link, which comes
# up well after the MQTT Ready probe. It gets the long budget; the remaining
# measurements publish in the same poll cycle, so once one flows the rest get the
# short budget.
echo "gate 2: serial link (fresh battery_pack)…"
for i in $(seq 36); do
  flux 'from(bucket:"telemetry") |> range(start:-60s) |> filter(fn:(r)=>r._measurement=="battery_pack") |> limit(n:1)' | grep -q battery_pack && break
  [ "$i" -eq 36 ] && { echo "::error::no fresh battery_pack rows after 180s — app is heartbeating but decoding no Seplos telemetry: socat<->sim serial link down or sim not answering 0x42 (see diagnostics step)"; exit 1; }
  sleep 5
done
# 24 x 5s = 120s: the battery_pack gate above can pass off PRE-rollout rows still inside its
# lookback window (the influx PVC persists across helm upgrades), while battery_cell needs the
# first POST-rollout PowerHardwareUpdate (10 s cadence + app/sim startup) — 60 s raced that on
# 2026-07-19 (rows were fresh ~30 s after the gate gave up).
for m in battery_cell dc_measurement; do
  for i in $(seq 24); do
    flux "from(bucket:\"telemetry\") |> range(start:-60s) |> filter(fn:(r)=>r._measurement==\"$m\") |> limit(n:1)" | grep -q "$m" && break
    [ "$i" -eq 24 ] && { echo "::error::no fresh $m rows after 120s while battery_pack flows — ingest mapping gap, not the link (see diagnostics step)"; exit 1; }
    sleep 5
  done
done
echo "fresh rows in all 4 steady-state measurements"

curl -sf "$GRAFANA/api/health" | grep -q '"database": *"ok"' && echo "grafana healthy"

# Fault injection — only AFTER fresh telemetry proved the app<->sim link is up: the sim
# rebuilds its state on every connection accept, so an injection that races the link
# bring-up is silently wiped (bit the first live run: exec'd 11 s after pod start, the
# sim accepted the connection after that, fault gone, assert timed out). The diagnostic
# check needs a FRESH row too — the standing bucket holds rows from prior injections.
"$KUBECTL" -n "$NS" exec "deploy/$RELEASE-sim" -- sh -c 'echo "faults high_voltage_protection" > /ctl/sim.ctl'
for i in $(seq 18); do
  flux 'from(bucket:"telemetry") |> range(start:-30s) |> filter(fn:(r)=>r._measurement=="diagnostic") |> limit(n:1)' | grep -q FMI_ && break
  [ "$i" -eq 18 ] && { echo "::error::diagnostic measurement never appeared after fault injection (90s)"; exit 1; }
  sleep 5
done
echo "fault injection surfaced in the diagnostic measurement"

# Standing environment: clear the injected fault so the live dashboards return to the
# healthy steady state (no teardown — the release keeps serving, docs.yml-style).
"$KUBECTL" -n "$NS" exec "deploy/$RELEASE-sim" -- sh -c 'echo "faults none" > /ctl/sim.ctl'
echo "## viz-uat: deploy + telemetry + fault-injection all green — standing env upgraded" >> "$GITHUB_STEP_SUMMARY"

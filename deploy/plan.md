# MQTT Visualization + Simulation Stack (Grafana / Helm / Docker)

## Context

The customer currently has to rely on AWS QuickSight to visualize telemetry. This change adds a self-contained, locally-runnable visualization + simulation stack to the repo: the existing Seplos battery **simulator** feeds the real **bin-rpi** app (serial poll → TMS mapping → embedded rumqttd MQTT broker), and the MQTT stream flows into **Telegraf → InfluxDB → Grafana** for full-history dashboards (a QuickSight replacement, with time-range queries — not just live view).

**Deliverable = files only**, committed and pushed to `claude/mqtt-grafana-visualization-j5h1g8`. The user builds/tests locally. **Zero Rust changes** — `bin-rpi` and `seplos-tcp-sim` are used as-is.

User-confirmed decisions:
1. Data path: Telegraf (mqtt_consumer) → InfluxDB v2 → Grafana (provisioned datasource + dashboards).
2. One multi-stage Dockerfile with per-service targets `app` and `sim`; Grafana/InfluxDB/Telegraf from official images configured via Helm.
3. Helm chart only — no docker-compose. *(Revised 2026-07-17 during verification:
   a docker-compose quickstart was added under `deploy/compose/` at the user's
   request — it reuses the chart's dashboards and mirrors its rendered configs;
   the Helm chart remains the Kubernetes path.)*

## Architecture

```
seplos-tcp-sim (--listen :5001, TCP)          [sim Deployment + Service]
        │ TCP
socat pty bridge ── /tmp/ttySIM (pty)         [same container as bin-rpi —
        │ serial (rs232)                       ptys cannot cross containers]
bin-rpi ── embedded rumqttd :1883 (0.0.0.0)   [app Deployment + Service]
        │ MQTT  tms/{device_id}/{topic}
Telegraf mqtt_consumer (json_v2)              [telegraf Deployment]
        │
InfluxDB v2 :8086 (PVC)                       [influxdb Deployment + PVC]
        │ Flux
Grafana :3000 (provisioned dashboards)        [grafana Deployment + Service]
```

Key verified constraints driving the design:
- `bin-rpi` has **no TCP serial transport** — only device paths (`kind = "rs232"|"rs485"`), so socat bridges pty↔TCP **inside the app container** (entrypoint script). Validated approach: `.claude/skills/run-bin-rpi/driver.py` already drives bin-rpi over a pty with `kind="rs232"`.
- `seplos-tcp-sim --listen` **accepts one connection and exits on disconnect** (`impl_core_connection.rs`: `Ok(0) => return Ok(())`). The sim entrypoint needs a re-listen loop, and the sim Deployment must have **NO tcp probes** (a probe would consume/kill the single accept slot).
- Sim runtime control (faults/state/behavior) is **stdin-only** → entrypoint feeds stdin from a FIFO at `/ctl/sim.ctl` so `kubectl exec … 'echo "faults …" > /ctl/sim.ctl'` works.
- `.cargo/config.toml` pins the internal kellnr mirror (`rust-mirror:8000`) — the Dockerfile must **NOT copy `.cargo/`** or off-network builds fail. No git deps in Cargo.lock, so crates.io works directly.
- A root `.dockerignore` would break `just docs-image` (root-context COPY of `book/`) — use BuildKit per-Dockerfile `deploy/docker/Dockerfile.dockerignore` instead.
- Workspace members include `bin/zephyr/platform` — builder stage must COPY it alongside `crates/`.
- TMS payload timestamps are `{"seconds": …}` objects, not RFC3339 — Telegraf uses message-arrival time.
- Config is `deny_unknown_fields` (`crates/bin/rpi/src/config/types/config.rs`) — rendered TOML must use exactly the keys from `config/sentient.toml` (incl. `parity`, `stop_bits`).

## New file tree

```
deploy/
├── README.md                      # what this is; disambiguates from distro/deploy (on-Pi assets)
├── docker/
│   ├── Dockerfile                 # multi-stage; targets: app, sim
│   ├── Dockerfile.dockerignore    # per-Dockerfile ignore (NOT root)
│   ├── app-entrypoint.sh          # socat pty bridge + exec bin-rpi
│   └── sim-entrypoint.sh          # FIFO control + re-listen loop
└── charts/sentient-viz/
    ├── Chart.yaml                 # v2, name sentient-viz, version 0.1.0, appVersion 0.2.3
    ├── values.yaml
    ├── .helmignore
    ├── dashboards/
    │   ├── battery-overview.json
    │   └── cell-detail.json
    └── templates/
        ├── _helpers.tpl  NOTES.txt
        ├── sim-deployment.yaml  sim-service.yaml
        ├── app-configmap.yaml  app-deployment.yaml  app-service.yaml
        ├── influxdb-secret.yaml  influxdb-pvc.yaml  influxdb-deployment.yaml  influxdb-service.yaml
        ├── telegraf-configmap.yaml  telegraf-deployment.yaml
        ├── grafana-secret.yaml  grafana-provisioning-configmap.yaml
        ├── grafana-dashboards-configmap.yaml  grafana-deployment.yaml  grafana-service.yaml
        └── networkpolicy.yaml     # {{- if .Values.networkPolicy.enabled }}
docs/deployment/grafana-viz.md     # mdbook runbook page
docs/SUMMARY.md                    # +1 link under Deployment
justfile                           # + [group('viz')] recipes
```

## Dockerfile (`deploy/docker/Dockerfile`)

- Builder: `rust:1.91-bookworm` (MSRV), `ARG TARGETARCH` mapped amd64→`x86_64-unknown-linux-musl`, arm64→`aarch64-unknown-linux-musl` (native musl builds, no cross toolchain; same flags as `just build-musl`: `-C target-feature=+crt-static`, rust-lld linker). Plain cargo + BuildKit cache mounts (no cargo-chef). `--locked --release`.
- Builds: `cargo build -p bin-rpi` and `cargo build -p test-kit --features tcp-sim --bin seplos-tcp-sim`.
- COPY only `Cargo.toml Cargo.lock crates/ bin/zephyr/platform/` — deliberately not `.cargo/` (kellnr mirror).
- Runtime `app` target: `alpine:3.20` + `socat` + `tini`; ENV `SIM_HOST=sim SIM_PORT=5001 PTY_LINK=/tmp/ttySIM RUST_LOG=info`; tini as ENTRYPOINT init (PID-1 SIGTERM handling).
- Runtime `sim` target: `alpine:3.20` + `tini`; ENV `SIM_PORT=5001 SIM_PACE=19200`.

`app-entrypoint.sh`: start `socat -d pty,raw,echo=0,link=$PTY_LINK tcp:$SIM_HOST:$SIM_PORT,connect-timeout=5,retry=120,interval=2` in background (pty exists while TCP retries; bin-rpi tolerates a silent bus via `read_timeout_ms`), wait-loop for the pty link, background watcher kills the container if socat dies (clean k8s restart), then `exec bin-rpi`.

`sim-entrypoint.sh`: `mkfifo /ctl/sim.ctl`; `( while :; do cat "$CTL"; done ) |` outer `while :` loop re-running `seplos-tcp-sim --listen $SIM_PORT --pace $SIM_PACE` (re-listen after each client disconnect; FIFO stays open across writers and restarts). Control grammar (from `impl_core_control_parse.rs`): `behavior normal|silence|corrupt-checksum|…`, `faults name,…|none`, `state sample14|fault-vector|key=value…`.

## Helm chart (`deploy/charts/sentient-viz/`)

Raw self-contained templates, **no subchart dependencies** (offline-friendly, 5 single-replica workloads). Standard `_helpers.tpl` fullname/labels + per-component `app.kubernetes.io/component` selector labels. Checksum annotations roll pods on ConfigMap/Secret change.

`values.yaml` surface (demo defaults, all overridable):
- `images.{app,sim,influxdb,telegraf,grafana}` — `sentient-viz-app:local`, `sentient-viz-sim:local`, `influxdb:2.7`, `telegraf:1.31`, `grafana/grafana:11.1.4`.
- `sim`: port 5001, pace 19200 (0 = instant), optional turnaroundMs.
- `app`: deviceId METEOR-001, poll {dipAddresses [0], intervalMs 2000, faultEvery 10}, brokerPort 1883, ptyLink, logLevel.
- `influxdb`: org sentient, bucket telemetry, retention "" (= infinite, full history), adminUser/adminPassword/adminToken (demo defaults, Secret-rendered), persistence {size 5Gi}.
- `telegraf`: flushIntervalSeconds 5.
- `grafana`: adminUser/adminPassword (admin/admin demo), service {type ClusterIP, port 3000}.
- `networkPolicy.enabled: false` (when true: app:1883 only from telegraf; sim:5001 only from app).
- resources requests/limits per component.

Template specifics:
- **sim**: replicas 1, `strategy: Recreate`, emptyDir at `/ctl`, **no probes** (single accept slot).
- **app ConfigMap** — rendered `sentient.toml` matching the `deny_unknown_fields` structs exactly:
  `[device] id`; `[serial] kind="rs232"` (pty has no RTS), `port={{ptyLink}}`, `baud=19200`, `parity="none"`, `stop_bits=1`, `read_timeout_ms=1000`; `[poll]` from values; `[broker] host="0.0.0.0"` (Telegraf reachability; plaintext/no-auth accepted for the demo stack), `port`; `[publish] prefix="tms"`.
- **app**: Recreate; env `SENTIENT_CONFIG=/etc/sentient/sentient.toml`, `SIM_HOST={{fullname}}-sim`, `SIM_PORT`, `PTY_LINK`, `RUST_LOG`; tcpSocket probes on 1883 (rumqttd tolerates probe connects, unlike the sim).
- **influxdb**: Deployment (1 replica) + explicit PVC at `/var/lib/influxdb2`; official-image init envs `DOCKER_INFLUXDB_INIT_MODE=setup`, `_USERNAME/_PASSWORD/_ADMIN_TOKEN` via secretKeyRef, `_ORG`, `_BUCKET`, `_RETENTION` only if set (setup is idempotent on an initialized volume); `httpGet /health :8086` probes.
- **telegraf**: telegraf.conf via subPath mount; env `MQTT_URL=tcp://{{fullname}}-app:1883`, `INFLUX_URL`, `INFLUX_ORG/BUCKET/TOKEN` (from influxdb Secret); no probes (both plugins retry internally).
- **grafana**: admin creds via Secret env; `INFLUX_TOKEN` env for `$__env{INFLUX_TOKEN}` in datasource provisioning; mounts: provisioning ConfigMap (datasources + dashboard provider), dashboards ConfigMap at `/var/lib/grafana/dashboards` via `(.Files.Glob "dashboards/*.json").AsConfig` (never through `tpl` — dashboard JSON may contain `{{ }}`), emptyDir at `/var/lib/grafana`; `httpGet /api/health` probes.
- **NOTES.txt**: port-forward, URL + creds, fault-injection one-liner, teardown.

## Telegraf config (in `telegraf-configmap.yaml`)

One `mqtt_consumer` per topic type (clean per-topic json_v2, distinct client_ids), arrival-time timestamps, `topic_parsing` extracting `device_id` from `tms/+/…`:
- `tms/+/dc_measurement_update` → measurement `dc_measurement`: object path `external_measurement`, tag `port_number`, fields `line_voltage/line_amperage/line_power` (nested `line` array flattens with `line_` prefix).
- `tms/+/power_hardware_update` → `battery_pack` (fields `energy_storage.{average,max,min}_temperature`) + `battery_cell` (object path `energy_storage.storage_cells`, tag `cell_number`, fields voltage / state_of_charge (0–1 fraction) / temperature / cycle_counter).
- `tms/+/active_diagnostic_state` → `diagnostic`: object path `codes`, tags `failure_mode`,`severity`, fields hint/occurrence_count/suspect_parameter.
- `tms/+/heartbeat` → `heartbeat`: field `sequence_number` (int).
- Output `influxdb_v2` with `${INFLUX_URL}/${INFLUX_TOKEN}/${INFLUX_ORG}/${INFLUX_BUCKET}`.
- Not ingested (documented): retained state topics (`device_info`, …) and `storage_update`.

## Grafana provisioning

- Datasource: InfluxDB, **Flux** query language (native to InfluxDB v2; InfluxQL would need imperative DBRP mapping that can't be provisioned declaratively), fixed `uid: influxdb-telemetry` referenced by every panel, `defaultBucket` from values (dashboards use `v.defaultBucket`), token via `$__env{INFLUX_TOKEN}`.
- Dashboard provider: file provider at `/var/lib/grafana/dashboards`, `allowUiUpdates: true`.
- Two hand-authored dashboard JSONs (schemaVersion ~39, Grafana 11.x). **Load the `dataviz` skill before authoring them** (colors/thresholds/layout):
  - `battery-overview.json`: pack V/A/W timeseries (unit overrides, amps on right axis); SOC gauge (`percentunit`, thresholds 0.2/0.5) + SOC trend; min/avg/max temperature; heartbeat count last-1m stat (≈60 healthy, red < 30).
  - `cell-detail.json`: 14-series cell-voltage timeseries; latest per-cell SOC bar gauge; per-cell temperatures; active-diagnostics table (Flux `pivot`, severity value-mappings).
- Expected steady-state numbers (sim `sample_14_cell`): ~53 V pack, +1.64 A (TMS source-positive; the vendor's −1.64 A after sign inversion), SOC 77.5 %, ~25 °C.

## justfile additions (`[group('viz')]`, matching existing style)

- `viz-images tag="local"` — `docker build -f deploy/docker/Dockerfile --target app|sim -t sentient-viz-{app,sim}:{{tag}} .`
- `viz-lint` — `helm lint` + `helm template` of the chart.
- `viz-install namespace="sentient-viz"` — `helm upgrade --install … --create-namespace`.
- `viz-uninstall namespace="sentient-viz"`.

## Docs (`docs/deployment/grafana-viz.md` + SUMMARY.md link)

Runbook: what/why; mermaid architecture diagram; build images (`just viz-images`, BuildKit + TARGETARCH note); install on kind (`kind load docker-image …`) / k3s / minikube; open Grafana (port-forward, admin/admin, what healthy looks like); **fault & state injection** via the FIFO, e.g.
`kubectl -n sentient-viz exec deploy/sentient-viz-sim -- sh -c 'echo "faults high_voltage_protection" > /ctl/sim.ctl'`, `state current=-3000`, `behavior silence` (note: faults surface on the next `fault_every` cycle, ≤ 20 s at defaults); topic→measurement data-model table; security caveats (plaintext no-auth broker, demo creds, `networkPolicy.enabled`); teardown.

## Verification

This environment has **no docker daemon and no helm** — pre-push verification is:
1. Download a pinned `helm` binary to the scratchpad; `rtk helm lint` + `helm template` (default values, plus `--set networkPolicy.enabled=true` and `--set influxdb.retention=30d` variants); review rendered manifests.
2. `python3 -m json.tool` on both dashboard JSONs; `tomllib` parse of rendered `telegraf.conf` + `sentient.toml`; cross-check TOML keys against the `deny_unknown_fields` config structs.
3. `sh -n` (+ shellcheck if available) on both entrypoint scripts.
4. `rtk git status` confirms zero Rust changes (no `just ci` gates triggered); mdbook SUMMARY link syntax check.
5. Explicitly deferred to the user's local run (documented in the docs page): `just viz-images` (docker build both targets), `kind load`, `just viz-install`, port-forward, dashboards populate ≤ 30 s, fault injection shows in the diagnostics table.

## Commits (on `claude/mqtt-grafana-visualization-j5h1g8`, pushed with `-u origin`)

1. `feat(deploy): multi-stage Dockerfile + entrypoints for bin-rpi app and seplos-tcp-sim` — `deploy/docker/*`, `deploy/README.md`.
2. `feat(deploy): sentient-viz Helm chart — sim + app + telegraf + influxdb + grafana` — chart tree minus dashboards.
3. `feat(deploy): provisioned Grafana dashboards (battery overview, cell detail)` — dashboard JSONs + ConfigMap wiring.
4. `docs(book): MQTT visualization stack runbook + just viz recipes` — docs page, SUMMARY.md, justfile. (CHANGELOG is git-cliff-generated — don't hand-edit.)

No PR unless requested.

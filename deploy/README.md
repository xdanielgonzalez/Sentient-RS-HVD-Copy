# deploy/ — container & Kubernetes demo assets

Cloud/container packaging for the **MQTT visualization + simulation stack**:
the `seplos-tcp-sim` battery simulator feeding the real `bin-rpi` adapter, with
telemetry flowing MQTT → Telegraf → InfluxDB → Grafana.

- `docker/` — one multi-stage Dockerfile with per-service targets (`app`, `sim`)
  plus their container entrypoints.
- `compose/` — docker-compose quickstart (no cluster needed): the same six
  services wired via compose, sharing the chart's dashboards. `just viz-up`.
- `charts/sentient-viz/` — self-contained Helm chart (no subchart dependencies)
  deploying sim + app + the Green Grass Bridge (mosquitto) + Telegraf +
  InfluxDB + Grafana. The Kubernetes path.

Not to be confused with `distro/deploy/`, which holds the **on-Pi** deployment
assets (systemd units, mosquitto bridge config) for the Yocto image.

Full runbook: `docs/deployment/grafana-viz.md` (build images, install on
kind/k3s/minikube, open Grafana, inject faults, teardown).

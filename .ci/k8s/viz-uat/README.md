# viz-uat — bring-up for the viz-stack continuous-deploy + UAT lane

`uat.yml`'s `viz-uat` job continuously deploys `deploy/charts/sentient-viz` onto the CI k3s
cluster (build-github), docs.yml-style: each qualifying merge to main `helm upgrade --install`s
the release, asserts real telemetry end-to-end (all four steady-state measurements land in
InfluxDB, Grafana is healthy, a FIFO fault injection surfaces in the `diagnostic` measurement,
then the fault is cleared) — and **leaves the stack running** as a standing Grafana demo with
full history on its PVC. Advisory lane — it does not gate the release auto-cut.

Access the standing environment:

```bash
ssh john@build-github
KUBECONFIG=/etc/rancher/k3s/k3s.yaml kubectl -n viz-uat port-forward svc/sentient-viz-grafana 3000:3000
# http://localhost:3000 — admin/admin (chart demo creds)
```

Until the one-time bring-up below is done, the job builds + pushes the images and then
**skips the install with a notice** (same pattern as the docs site's `DOCS_KUBECONFIG`, #294).

## One-time bring-up (on build-github)

```bash
export KUBECONFIG=/etc/rancher/k3s/k3s.yaml     # NOT ~/k3s.yaml (dead hvd-e1)
kubectl apply -f namespace.yaml -f rbac.yaml
./setup-deployer-kubeconfig.sh | gh secret set VIZ_UAT_KUBECONFIG
```

## Guardrails on the shared cluster

- **Fixed namespace + fixed release name** (`viz-uat` / `sentient-viz`): uat.yml is already
  serialized (`concurrency: uat-<ref>`), so runs never race and `helm upgrade --install`
  makes every merge an idempotent upgrade of the ONE standing release.
- **One bounded PVC**: the standing influxdb keeps its 5Gi claim (the history *is* the demo);
  set `influxdb.retention` if the node's disk pressure ever makes infinite history a problem.
- **Scoped credential**: `viz-uat-deployer` is namespace-admin over viz-uat only — it cannot
  create namespaces or touch anything else on the cluster.
- **Images**: pushed to Harbor as `rust-mirror:443/sentient/sentient-viz-{app,sim}:uat-<run_id>`.
  Run-scoped tags are tiny (~15 MB compressed each); add a Harbor retention policy on the
  `uat-*` pattern if they ever bother anyone.

## Rotation

Delete the token Secret and re-run the script (it re-mints and re-prints):

```bash
kubectl -n viz-uat delete secret viz-uat-deployer-token
./setup-deployer-kubeconfig.sh | gh secret set VIZ_UAT_KUBECONFIG
```

## Tailnet exposure (Grafana on Tailscale)

The chart carries an opt-in `tailscale serve` sidecar on the grafana pod (mirrors the docs-site
pattern, `.ci/k8s/docs/`): machine `sentient-viz`, HTTPS to tailnet members only (no Funnel).
The UAT lane enables it AUTOMATICALLY on the next merge once bring-up has run — it checks for
the auth (or existing state) Secret and adds `--set tailscale.enabled=true
--set tailscale.rbac.create=false` only then, so pre-bring-up merges deploy unchanged.

One-time bring-up (cluster admin):

```bash
# 1. rbac.yaml already carries the pod-SA state Role/RoleBinding — re-apply after pulling:
kubectl apply -f rbac.yaml

# 2. Mint a REUSABLE, PRE-AUTHORIZED auth key in the Tailscale admin console
#    (Settings -> Keys; tag it if your ACLs use tags) and store it:
kubectl -n viz-uat create secret generic sentient-viz-tailscale-auth \
  --from-literal=TS_AUTHKEY=tskey-auth-XXXXXXXXX

# 3. Next merge to main (or a manual uat.yml dispatch) deploys the sidecar and joins the
#    tailnet. Then: http://sentient-viz.<tailnet>.ts.net (https once the tailnet HTTPS-certs feature is on)
#    The auth Secret may be deleted after the first successful join — the node identity
#    persists in the sentient-viz-tailscale-state Secret.
```

Grafana's committed demo password is now reachable by every tailnet member — override
`grafana.adminPassword` (and the Influx credentials) before or at bring-up.

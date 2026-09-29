#!/usr/bin/env bash
#
# Mint the CI credential for the viz-stack UAT lane: a long-lived ServiceAccount token for
# `viz-uat-deployer` (Role: viz-uat namespace admin — see rbac.yaml) wrapped in a minimal
# kubeconfig, printed to stdout for `gh secret set VIZ_UAT_KUBECONFIG`.
#
# Run ON build-github (the k3s node) — no sudo needed, k3s.yaml is world-readable there:
#   kubectl apply -f namespace.yaml -f rbac.yaml
#   KUBECONFIG=/etc/rancher/k3s/k3s.yaml ./setup-deployer-kubeconfig.sh | gh secret set VIZ_UAT_KUBECONFIG
#
# SERVER: the API endpoint the ARC runner pods will dial. 127.0.0.1 is only valid on the node
# itself, so the kubeconfig is written with the node's cluster-reachable address instead
# (default below = build-github's node IP — NOT rust-mirror, which is Harbor, a different host;
# override: SERVER=https://<ip>:6443 ./setup-deployer-kubeconfig.sh).
set -euo pipefail

SERVER="${SERVER:-https://10.0.0.67:6443}"
NS=viz-uat
SA=viz-uat-deployer
TOKEN_SECRET=viz-uat-deployer-token

# Manifests must be applied first (namespace + SA + Role); this only mints the token.
kubectl -n "$NS" get serviceaccount "$SA" >/dev/null

# A v1 Secret of type service-account-token gets a non-expiring token provisioned by the
# controller — the right shape for a CI secret (rotate by deleting the Secret and re-running).
kubectl -n "$NS" apply -f - >/dev/null <<EOF
apiVersion: v1
kind: Secret
metadata:
  name: ${TOKEN_SECRET}
  annotations:
    kubernetes.io/service-account.name: ${SA}
type: kubernetes.io/service-account-token
EOF

# Token/CA provisioning is asynchronous — poll briefly.
for _ in $(seq 1 30); do
  TOKEN=$(kubectl -n "$NS" get secret "$TOKEN_SECRET" -o jsonpath='{.data.token}' 2>/dev/null | base64 -d || true)
  [ -n "$TOKEN" ] && break
  sleep 1
done
[ -n "${TOKEN:-}" ] || { echo "error: token was not provisioned onto ${TOKEN_SECRET}" >&2; exit 1; }
CA=$(kubectl -n "$NS" get secret "$TOKEN_SECRET" -o jsonpath='{.data.ca\.crt}')

cat <<EOF
apiVersion: v1
kind: Config
clusters:
  - name: sentient-k3s
    cluster:
      server: ${SERVER}
      certificate-authority-data: ${CA}
contexts:
  - name: viz-uat-deployer
    context:
      cluster: sentient-k3s
      namespace: ${NS}
      user: ${SA}
current-context: viz-uat-deployer
users:
  - name: ${SA}
    user:
      token: ${TOKEN}
EOF

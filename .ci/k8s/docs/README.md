# Docs site on the tailnet — one-time setup

The rendered mdbook ships as an nginx image (`.ci/docs.Dockerfile` → `rust-mirror:443/sentient/sentient-docs`)
and runs on the build-github k3s cluster with a Tailscale sidecar that serves
**https://sentient-docs.\<tailnet\>.ts.net** to tailnet members only (`AllowFunnel: false` — never
public). CI (`docs.yml`) pushes a fresh image and rollout-restarts the deployment on every merge
to main. Everything below is a ONE-TIME bring-up, run on build-github with
`KUBECONFIG=/etc/rancher/k3s/k3s.yaml`.

## 1. Tailnet prerequisites (admin console)

- **DNS → MagicDNS**: enabled.
- **DNS → HTTPS Certificates**: needed only for the **https://** URL — tailscale serve can't mint
  the `*.ts.net` cert without it (containerboot logs "not able to issue TLS certs", `tailscale
  cert` says "your Tailscale account does not support getting TLS certs").

**The site works over plain HTTP without that toggle** (verified 2026-07-17): in userspace mode,
tailscaled forwards inbound tailnet connections to the same port on localhost, so
`http://sentient-docs.<tailnet>.ts.net` reaches nginx directly. Transport is still
WireGuard-encrypted end to end — the HTTP leg exists only inside the tunnel. Enabling HTTPS
Certificates (then bouncing the pod so the serve config re-applies) upgrades it to a
browser-trusted `https://` URL.

## 2. Mint the sidecar auth key

Admin console → Settings → Keys → **Generate auth key**:

- **Reusable**: yes (a Recreate rollout may re-register before state settles).
- **Ephemeral**: no — the node identity persists in the `sentient-docs-tailscale-state` Secret;
  an ephemeral node would be garbage-collected on restart gaps.
- **Tags**: recommended (e.g. `tag:server`) — tagged nodes have no key-expiry clock.

Then create the (short-lived) enrollment Secret:

```sh
kubectl -n docs create secret generic sentient-docs-tailscale-auth \
  --from-literal=TS_AUTHKEY='tskey-auth-…'
```

After the machine shows up in the admin console you may delete this Secret — the pod mounts it
`optional: true` and `TS_AUTH_ONCE=true` means it is only read when no state exists.

## 3. Node containerd trust for Harbor

The kubelet pulls `rust-mirror:443/sentient/sentient-docs` directly (not through dind). **On the
current nodes this already works with no registries.yaml entry for rust-mirror:443** (verified
2026-07-17: the pull succeeded on build-github-worker as-is — the nodes evidently trust the
Harbor cert through their system store). Verify rather than configure:

```sh
kubectl -n docs get pods    # nginx pulling = trust is fine
```

Only if a pull fails with an x509/TLS error, add to `/etc/rancher/k3s/registries.yaml` on the
affected node and restart k3s:

```yaml
configs:
  "rust-mirror:443":
    tls:
      ca_file: /etc/rancher/k3s/harbor-ca.crt   # copy of .ci/harbor-ca.crt
```

## 4. First image, then apply

The deployment pulls `:latest`, so publish an image before applying — merge a change that
triggers `docs.yml` on main (or re-run its latest main run) and wait for the `publish` job. Then:

```sh
kubectl apply -f .ci/k8s/docs/namespace.yaml
kubectl apply -f .ci/k8s/docs/rbac.yaml
kubectl apply -f .ci/k8s/docs/serve-config.yaml
kubectl apply -f .ci/k8s/docs/deployment.yaml
kubectl -n docs get pods -w        # both containers Ready; tailscale readiness = has tailnet IP
```

## 5. CI rollout credential

```sh
sudo KUBECONFIG=/etc/rancher/k3s/k3s.yaml ./setup-deployer-kubeconfig.sh | gh secret set DOCS_KUBECONFIG
```

`docs-deployer` can do exactly one thing: `rollout restart` / `rollout status` on this deployment
(see `rbac.yaml`). Rotate by deleting the `docs-deployer-token` Secret and re-running.

## 6. Verify

Open **https://sentient-docs.\<tailnet\>.ts.net** from any tailnet device (first hit may take a
few seconds while the cert is minted). From off-tailnet it must not resolve/connect. After the
next merge to main, confirm the site shows the new content (CI restarted the pod).

## Rollback

`kubectl -n docs set image deployment/sentient-docs nginx=rust-mirror:443/sentient/sentient-docs:<sha>`
— every merge's image is also pushed with its git SHA tag. Roll forward by restarting back onto
`:latest`.

# Docs-site image: nginx serving the rendered mdbook site (book/) over plain HTTP on :80.
# TLS + access control are deliberately NOT this container's job — the Tailscale sidecar in
# .ci/k8s/docs/deployment.yaml terminates HTTPS on the tailnet (tailscale serve) and proxies to
# 127.0.0.1:80, so the only reachable surface is tailnet members. AllowFunnel is false: never public.
#
# Build context is the repo root and EXPECTS book/ to be prebuilt (`just docs`); CI builds it from
# the docs-site artifact in .github/workflows/docs.yml. Locally: `just docs && just docs-image`.
#
# Digest-pinned (resolved 2026-07-17 via registry API): nginx:stable-alpine.
FROM nginx:stable-alpine@sha256:ec664813a30459a8e7176315268a623f6b31abc370eeac51c7de81cd4ec4d451

COPY .ci/k8s/docs/nginx.conf /etc/nginx/conf.d/default.conf
COPY book/ /usr/share/nginx/html/

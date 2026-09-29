#!/usr/bin/env bash
#
# Build + push the sentient-builder + sentient-yocto-builder images to
# GitHub Container Registry (ghcr.io/high-velocity-decisions). The pipeline pulls them back
# through the Harbor pull-through cache — `rust-mirror:443/sentient/...` —
# the same Harbor pull-through path the CI jobs use. ghcr.io has valid public TLS, so there is no local
# cert/insecure-registry setup to do.
#
# FALLBACK PATH: the primary way to publish these images is now
# .github/workflows/builder-image.yml (CI-built + pushed on arc-runner-set, triggered by
# workflow_dispatch or a push touching ci/**). This script remains for local/offline use when CI
# isn't available or you're iterating on a Dockerfile before pushing.
#
# Credentials are USED, never STORED: GHCR_USERNAME (your GitHub username) and GHCR_TOKEN (a PAT
# with `write:packages`) are read from the environment for a one-shot `docker login`; the script
# logs out on exit (nothing left in ~/.docker/config.json, nothing written to GitHub secrets).
#
# Needs a local Docker daemon. The image BUILD still pulls crates through the internal cargo mirror
# (host `rust-mirror`, the box also hosting Harbor), so your machine must be able to resolve/reach
# `rust-mirror` too.
#
# Usage:
#   GHCR_USERNAME=you GHCR_TOKEN=ghp_… ./.ci/scripts/publish-builder-image.sh
#   GHCR_USERNAME=you GHCR_TOKEN=ghp_… ./.ci/scripts/publish-builder-image.sh <namespace> <registry> <tag>
#
set -euo pipefail

NAMESPACE="${1:-${GHCR_NAMESPACE:-high-velocity-decisions}}"
REGISTRY="${2:-${GHCR_REGISTRY:-ghcr.io}}"
TAG="${3:-${TAG:-latest}}"

: "${GHCR_USERNAME:?set GHCR_USERNAME (your GitHub username)}"
: "${GHCR_TOKEN:?set GHCR_TOKEN (a GitHub PAT with write:packages)}"
command -v docker >/dev/null 2>&1 || { echo "error: docker not found (need a local Docker daemon)"; exit 1; }

REG="${REGISTRY}/${NAMESPACE}"
# The baked .cargo mirror points at host `rust-mirror` (the box hosting both Harbor and the kellnr
# crates mirror); local Docker builds have no DNS for it, so map it to the real IP during build via
# --add-host so cargo resolves it. Override with KELLNR_HOST=host:ip.
KELLNR_HOST="${KELLNR_HOST:-rust-mirror:10.0.0.192}"

cleanup() { docker logout "${REGISTRY}" >/dev/null 2>&1 || true; }
trap cleanup EXIT

echo "→ docker login ${REGISTRY} (ephemeral — logged out on exit)"
printf '%s' "${GHCR_TOKEN}" | docker login "${REGISTRY}" -u "${GHCR_USERNAME}" --password-stdin

echo "→ build + push ${REG}/sentient-builder:${TAG}"
docker build -f .ci/Dockerfile --add-host "${KELLNR_HOST}" -t "${REG}/sentient-builder:${TAG}" .
docker push "${REG}/sentient-builder:${TAG}"

echo "→ build + push ${REG}/sentient-yocto-builder:${TAG}  (FROM sentient-builder, local)"
docker build -f .ci/yocto.Dockerfile --add-host "${KELLNR_HOST}" \
  --build-arg BASE_IMAGE="${REG}/sentient-builder:${TAG}" \
  -t "${REG}/sentient-yocto-builder:${TAG}" .
docker push "${REG}/sentient-yocto-builder:${TAG}"

echo "✓ pushed ${REG}/sentient-builder:${TAG} + sentient-yocto-builder:${TAG}"
echo "  CI pulls via the Harbor proxy: rust-mirror:443/${NAMESPACE}/sentient-builder:${TAG}"
echo "  Primary path is now .github/workflows/builder-image.yml (CI-built, no local Docker needed);"
echo "  this script remains the local/offline fallback."

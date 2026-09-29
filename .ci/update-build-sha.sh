#!/usr/bin/env bash
# ci/update-build-sha.sh — pin the derived builder Dockerfiles' base to the current GHCR digest.
#
# The derived image (sentient-yocto-builder) FROM sentient-builder via a
# default `ARG BASE_IMAGE=<registry>/sentient-builder:latest`. `:latest` is MUTABLE, and the Harbor
# pull-through proxy (rust-mirror:443/ghcr/...) caches mutable tags — so a build off the default can
# silently pick up a STALE base (this is exactly why release.yml pins those `container:` refs by
# @sha256 digest). This script resolves the CURRENT digest of sentient-builder in GHCR and rewrites
# each derived Dockerfile's `ARG BASE_IMAGE=` default to the immutable `<registry>/sentient-builder@
# sha256:...` form, preserving whatever registry prefix the file already uses.
#
# Usage:
#   ci/update-build-sha.sh [--tag <tag>] [--dry-run] [--check]
#     --tag <tag>   GHCR tag to resolve the digest from (default: latest).
#     --package <p> Image to resolve (default: sentient-builder). Mainly for testing the resolver.
#     --dry-run     Resolve + print the digest and the edits, but write nothing.
#     --check       Exit non-zero if any file is NOT already pinned to the current digest.
#                   For a CI drift gate. Implies --dry-run.
#
# Resolution order (first that yields a digest wins):
#   1. Harbor ghcr registry-v2 manifest HEAD — the PREFERRED path. It reads the digest from the
#      SAME cache the Dockerfiles pull through, so we pin exactly what the proxy will serve; uses an
#      explicit --cacert (.ci/harbor-ca.crt) so it needs no docker-daemon / node CA trust; and needs
#      no GH `read:packages` scope (the proxy issues an anonymous pull token). This is the path CI +
#      an on-network dev box both use.
#   2. gh packages API — GHCR-direct fallback, needs a token with `read:packages` (CI's GITHUB_TOKEN
#      with `packages: read` has it). Sees private images.
#   3. GHCR registry-v2 anonymous manifest HEAD — last resort for PUBLIC images off-network.
#
# Note: a registry serves the top-level manifest digest, which is the correct thing to pin a FROM to.
# Harbor proxies GHCR's manifest bytes verbatim, so the proxy digest == the upstream GHCR digest.
set -euo pipefail

HARBOR_HOST="rust-mirror:443"
HARBOR_PROJECT="sentient"
ORG="high-velocity-decisions"
PACKAGE="sentient-builder"
CA_CERT=".ci/harbor-ca.crt"   # committed Harbor private-CA cert; relative to repo root (we cd there).
TAG="latest"
DRY_RUN=0
CHECK=0

# Dockerfiles whose `ARG BASE_IMAGE=.../sentient-builder(:tag|@sha256:..)` default gets pinned.
FILES=(
  ".ci/yocto.Dockerfile"
)

while [[ $# -gt 0 ]]; do
  case "$1" in
    --tag)     TAG="${2:?--tag needs a value}"; shift 2 ;;
    --package) PACKAGE="${2:?--package needs a value}"; shift 2 ;;
    --dry-run) DRY_RUN=1; shift ;;
    --check)   CHECK=1; DRY_RUN=1; shift ;;
    -h|--help)
      sed -n '2,26p' "$0" | sed 's/^# \{0,1\}//'
      exit 0 ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done

# Move to repo root so the relative FILES paths resolve regardless of the caller's cwd.
cd "$(git rev-parse --show-toplevel)"

# Parse the `.token` field from a registry token endpoint's JSON (jq if present, else python3).
json_token() {
  if command -v jq >/dev/null 2>&1; then jq -r '.token // empty'
  else python3 -c 'import sys,json;print(json.load(sys.stdin).get("token",""))'; fi
}

# The Accept set that asks a registry for the manifest/index digest without fetching layers.
MANIFEST_ACCEPT="application/vnd.oci.image.index.v1+json,application/vnd.docker.distribution.manifest.list.v2+json,application/vnd.oci.image.manifest.v1+json,application/vnd.docker.distribution.manifest.v2+json"

# HEAD a manifest at ${1}=host, ${2}=repo path, ${3}=bearer token, extra curl args after; print the
# Docker-Content-Digest header value (sha256:...) on success only.
manifest_digest() {
  local host="$1" repo="$2" token="$3"; shift 3
  curl -fsSI --max-time 20 "$@" -H "Authorization: Bearer ${token}" -H "Accept: ${MANIFEST_ACCEPT}" \
    "https://${host}/v2/${repo}/manifests/${TAG}" 2>/dev/null \
    | tr -d '\r' | awk 'tolower($1)=="docker-content-digest:"{print $2}'
}

# 1. Preferred: hosted Harbor project — read the digest from the same project the Dockerfiles pull.
#    The hosted `sentient` project path is `<project>/<package>` (NO org segment — the org only
#    appeared in the old ghcr-PROXY path, which mirrored ghcr.io/<org>/...). Override HARBOR_REPO for
#    a proxy-style path that includes the org.
resolve_via_harbor() {
  command -v curl >/dev/null 2>&1 || return 1
  local repo="${HARBOR_REPO:-${HARBOR_PROJECT}/${PACKAGE}}" token out; local ca=()
  [[ -f "${CA_CERT}" ]] && ca=(--cacert "${CA_CERT}")
  token="$(curl -fsS --max-time 20 "${ca[@]}" \
    "https://${HARBOR_HOST}/service/token?service=harbor-registry&scope=repository:${repo}:pull" 2>/dev/null \
    | json_token)" || return 1
  [[ -n "${token}" ]] || return 1
  out="$(manifest_digest "${HARBOR_HOST}" "${repo}" "${token}" "${ca[@]}")" || return 1
  [[ "${out}" == sha256:* ]] || return 1
  printf '%s\n' "${out}"
}

# 2. Fallback: gh packages API (needs read:packages; sees private images). gh leaks the error JSON
#    body to stdout on a 403, so validate the sha256 prefix before trusting it.
resolve_via_gh_api() {
  command -v gh >/dev/null 2>&1 || return 1
  local out
  out="$(gh api --paginate -H "Accept: application/vnd.github+json" \
    "/orgs/${ORG}/packages/container/${PACKAGE}/versions" \
    --jq "[.[] | select(.metadata.container.tags[]? == \"${TAG}\")][0].name // empty" 2>/dev/null)" || return 1
  [[ "${out}" == sha256:* ]] || return 1
  printf '%s\n' "${out}"
}

# 3. Last resort: GHCR registry-v2 anonymous manifest HEAD (public images, off-network).
resolve_via_ghcr() {
  command -v curl >/dev/null 2>&1 || return 1
  local token out
  token="$(curl -fsS --max-time 20 "https://ghcr.io/token?service=ghcr.io&scope=repository:${ORG}/${PACKAGE}:pull" 2>/dev/null \
    | json_token)" || return 1
  [[ -n "${token}" ]] || return 1
  out="$(manifest_digest "ghcr.io" "${ORG}/${PACKAGE}" "${token}")" || return 1
  [[ "${out}" == sha256:* ]] || return 1
  printf '%s\n' "${out}"
}

# Derived (child) images and the workflow files whose `container:` refs pin them. The base image
# (sentient-builder) is pinned INTO the two derived Dockerfiles above (FILES); these derived images
# are pinned INTO the workflow yamls' `container:` — the "digest in the yaml". A `:latest` container
# ref is mutable (and the Harbor proxy caches mutable tags), so a job can silently boot a stale image;
# pinning by @sha256 makes every run boot the EXACT image this script resolved.
DERIVED=(
  "sentient-yocto-builder"
)
WORKFLOWS=(
  ".github/workflows/_yocto.yml"
  ".github/workflows/_uat-qemu.yml"
  ".github/workflows/_release-sysext.yml"
)
# The BASE image (sentient-builder) ALSO runs directly as the `container:` in the fast-lane + release
# workflows (not only as a Dockerfile base). Those refs must be pinned by @sha256 too — a mutable
# :latest there is exactly what lets the Harbor proxy serve a stale builder. builder-image.yml is
# EXCLUDED: it PUSHES sentient-builder:latest (the producer), so its :latest tag is intentional.
BASE_WORKFLOWS=(
  ".github/workflows/_lint.yml"
  ".github/workflows/_test.yml"
  ".github/workflows/_quality.yml"
  ".github/workflows/_security.yml"
  ".github/workflows/_supply-chain.yml"
  ".github/workflows/scheduled.yml"
  ".github/workflows/production.yml"
  ".github/workflows/_release-build.yml"
  ".github/workflows/_release-publish.yml"
  ".github/workflows/_uat-qemu.yml"
  ".github/workflows/docs.yml"
  # Added with #377: the MCU firmware lane also boots sentient-builder directly, and it is the lane
  # that needs the NEXT image rebuild (the one baking gcc-arm-none-eabi for the RP2350 S-world CMSE
  # veneer). Without it here, `--update` would leave this file behind on the old digest and `--check`
  # would never notice. (_dds_interop.yml is still missing from this list — same gap, separate fix.)
  ".github/workflows/_mcu-build.yml"
)

# resolve_digest <package> -> echoes sha256:... (current tag digest), or returns 1. Sets PACKAGE (the
# resolve_via_* helpers read it) — callers pass an explicit name so the global is always well-defined.
resolve_digest() {
  PACKAGE="$1"
  local dg
  dg="$(resolve_via_harbor || true)"
  [[ "${dg}" == sha256:* ]] || dg="$(resolve_via_gh_api || true)"
  [[ "${dg}" == sha256:* ]] || dg="$(resolve_via_ghcr || true)"
  [[ "${dg}" == sha256:* ]] || return 1
  printf '%s\n' "${dg}"
}

# pin_file <file> <sed_expr> <grep_regex> <label> — rewrite every line matching grep_regex via sed_expr,
# honoring --check (report drift, rc=1) / --dry-run (report only) / write. Tracks global rc + drift.
pin_file() {
  local f="$1" se="$2" mg="$3" label="$4" cur new
  [[ -f "$f" ]] || { echo "warn: ${f} not found, skipping" >&2; return 0; }
  cur="$(grep -E "$mg" "$f" || true)"
  [[ -n "$cur" ]] || { echo "warn: ${f} has no ${label} ref, skipping" >&2; return 0; }
  new="$(printf '%s\n' "$cur" | sed -E "$se")"
  if [[ "$cur" == "$new" ]]; then
    echo "  ok    ${f} (${label} already current)"
    return 0
  fi
  drift=1
  if [[ "${CHECK}" == 1 ]]; then
    rc=1; echo "  DRIFT ${f} (${label})"
    printf '        - %s\n' "$cur"; printf '        + %s\n' "$new"
  elif [[ "${DRY_RUN}" == 1 ]]; then
    echo "  would ${f} (${label})"
    printf '        - %s\n' "$cur"; printf '        + %s\n' "$new"
  else
    sed -E -i "$se" "$f"
    echo "  wrote ${f} (${label})"
  fi
  return 0
}

rc=0
drift=0

# 1) BASE image (sentient-builder) -> the two derived Dockerfiles' `ARG BASE_IMAGE=` default.
echo "Resolving ${PACKAGE}:${TAG} (base) digest (Harbor ${HARBOR_HOST}/${HARBOR_PROJECT})..." >&2
if BASE_DIGEST="$(resolve_digest "${PACKAGE}")"; then
  echo "  ${PACKAGE}:${TAG} = ${BASE_DIGEST}" >&2
  for f in "${FILES[@]}"; do
    pin_file "$f" \
      "s#^(ARG BASE_IMAGE=[^[:space:]]*/${PACKAGE})(:[[:alnum:]._-]+|@sha256:[0-9a-f]+)#\1@${BASE_DIGEST}#" \
      "^ARG BASE_IMAGE=[^[:space:]]*/${PACKAGE}" "base ${PACKAGE}"
  done
else
  echo "error: could not resolve a digest for the base ${PACKAGE}:${TAG}." >&2
  echo "       Is the image pushed yet? On-network needs the Harbor CA (${CA_CERT}); GHCR-direct" >&2
  echo "       needs gh authed with read:packages (private) or a public package." >&2
  exit 1
fi

# 1b) BASE image also -> its OWN `container:` refs in the fast-lane + release workflows (reuse
#     BASE_DIGEST from above — same image, same digest as the Dockerfile base).
for wf in "${BASE_WORKFLOWS[@]}"; do
  pin_file "$wf" \
    "s#(container:[[:space:]]*[^[:space:]]*/${PACKAGE})(:[[:alnum:]._-]+|@sha256:[0-9a-f]+)#\1@${BASE_DIGEST}#" \
    "container:[[:space:]]*[^[:space:]]*/${PACKAGE}" "container ${PACKAGE}"
done

# 2) DERIVED images -> the workflow yamls' `container:` refs (the "digest in the yaml").
for img in "${DERIVED[@]}"; do
  echo "Resolving ${img}:${TAG} (derived) digest..." >&2
  if DG="$(resolve_digest "${img}")"; then
    echo "  ${img}:${TAG} = ${DG}" >&2
    for wf in "${WORKFLOWS[@]}"; do
      pin_file "$wf" \
        "s#(container:[[:space:]]*[^[:space:]]*/${img})(:[[:alnum:]._-]+|@sha256:[0-9a-f]+)#\1@${DG}#" \
        "container:[[:space:]]*[^[:space:]]*/${img}" "container ${img}"
    done
  else
    echo "warn: could not resolve ${img}:${TAG}; leaving its yaml container refs as-is." >&2
  fi
done

if [[ "${CHECK}" == 1 ]]; then
  if [[ "${rc}" == 0 ]]; then
    echo "All base + container refs pinned to current digests." >&2
  else
    echo "One or more refs are out of date; run '.ci/update-build-sha.sh' to pin." >&2
  fi
  exit "${rc}"
fi

if [[ "${DRY_RUN}" == 0 && "${drift}" == 1 ]]; then
  echo "Done. Review + commit the pinned Dockerfiles + workflows." >&2
fi

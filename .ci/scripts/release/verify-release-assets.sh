#!/usr/bin/env bash
# .ci/scripts/release/verify-release-assets.sh — hard gate on the ACTUAL published release state.
#
# The softprops publish step is continue-on-error because it can exit non-zero on a transient
# concurrent-upload metadata race even when the release + every asset is present (observed on run
# 29447780490). This script fails the job iff the release is genuinely missing or short a required
# deliverable — so a real publish failure is never masked, but a cosmetic race never reds a
# complete release.
#
# The required-asset set lives in .ci/release-required-assets.txt — the SAME list promote.yml
# checks before promoting a candidate, so the two gates can no longer drift apart.
#
# Env contract: GH_TOKEN, TAG (vX.Y.Z), REPO (owner/repo), GITHUB_WORKSPACE.
set -euo pipefail

# curl (not gh/jq — sentient-builder bakes neither) against the GitHub REST API. A missing
# release -> non-200 -> fail. The repo lives on github.com even though the runner is
# self-hosted, so api.github.com is the right host.
code=$(curl -sS -o /tmp/rel.json -w '%{http_code}' \
  -H "Authorization: Bearer $GH_TOKEN" \
  -H "Accept: application/vnd.github+json" \
  -H "X-GitHub-Api-Version: 2022-11-28" \
  "https://api.github.com/repos/$REPO/releases/tags/$TAG")
[ "$code" = 200 ] || { echo "::error::release $TAG not found (HTTP $code)"; head -c 800 /tmp/rel.json; echo; exit 1; }
# Strip whitespace so `"name": "x"` and `"name":"x"` match uniformly; asset filenames have no
# spaces, and the release TITLE ("sentient-rs vX") can't collide with an asset filename.
tr -d ' \t' < /tmp/rel.json > /tmp/rel.min
missing=0
while IFS= read -r f; do
  # Skip comments and blank lines in the required-assets list.
  case "$f" in ''|'#'*) continue ;; esac
  grep -qF "\"name\":\"$f\"" /tmp/rel.min || { echo "::error::release $TAG missing required asset: $f"; missing=1; }
done < "$GITHUB_WORKSPACE/.ci/release-required-assets.txt"
[ "$missing" = 0 ] || { echo "::error::release $TAG is incomplete — failing the job"; exit 1; }
echo "::notice::release $TAG verified complete — all required assets present"

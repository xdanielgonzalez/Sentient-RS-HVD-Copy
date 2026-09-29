#!/usr/bin/env bash
# .ci/scripts/uat/pull-golden-base.sh — pull the GOLDEN BASE image bundle from MinIO.
#
# Pull IN-CLUSTER (fast) instead of downloading the pre-release asset over the internet from
# GitHub (which was timing out). image.yml (_yocto.yml) stages the same bundle to a fixed MinIO
# key on every golden build; the GitHub pre-release stays the durable, human-browsable copy.
# mc is baked into sentient-yocto-builder. Post-split, this golden is the APP-FREE base —
# sentient-image no longer bakes in bin-rpi at all.
#
# Env contract: MACHINE, MC_ENDPOINT, MC_KEY, MC_SECRET.
set -euo pipefail

command -v mc >/dev/null 2>&1 || { echo "::error::mc not in the yocto builder image"; exit 1; }
mc alias set minio "$MC_ENDPOINT" "$MC_KEY" "$MC_SECRET" >/dev/null 2>&1 \
  || { echo "::error::mc alias set failed (MinIO creds?)"; exit 1; }
SRC="minio/cache/golden/${MACHINE}/sentient-image.tar.gz"
mc stat "$SRC" >/dev/null 2>&1 \
  || { echo "::error::no golden image in MinIO at $SRC — run image.yml (workflow_dispatch) once to build + stage the golden, then re-run UAT"; exit 1; }
mkdir -p dl
mc cp "$SRC" "dl/sentient-image.tar.gz"
echo "::notice::pulled golden base from MinIO: $SRC ($(du -h dl/sentient-image.tar.gz | cut -f1))"
ls -lh dl/sentient-image.tar.gz

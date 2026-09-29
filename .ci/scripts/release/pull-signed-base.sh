#!/usr/bin/env bash
# .ci/scripts/release/pull-signed-base.sh — pull the pre-signed BASE .wic from the MinIO golden
# store that image.yml/_yocto.yml publishes.
#
# Mirrors the `mc cp` pattern the UAT lane uses to pull its qemuarm64 golden. The sysext job does
# not need the base .wic to BUILD the sysext (the sysext recipe is independent of the base rootfs),
# but the base+sysext PAIR is what the release publishes together, so the sysext job is the natural
# place to fetch it for this MACHINE and hand it downstream as a same-name artifact.
#
# TODO(verify-at-build): confirm the golden `.wic` MinIO key convention set by _yocto.yml
# (minio/cache/golden/<machine>/sentient-image.wic(.bz2) + .p7s) against a real staged object —
# authored to match _yocto.yml's own staging step, but neither side has run yet.
#
# Env contract: MACHINE, MC_ENDPOINT, MC_KEY, MC_SECRET. Output lands in base-out/.
set -euo pipefail

command -v mc >/dev/null 2>&1 || { echo "::error::mc not in the yocto builder image"; exit 1; }
mc alias set minio "$MC_ENDPOINT" "$MC_KEY" "$MC_SECRET" >/dev/null 2>&1 \
  || { echo "::error::mc alias set failed (MinIO creds?)"; exit 1; }
mkdir -p base-out
FOUND=0
for ext in wic.bz2 wic; do
  SRC="minio/cache/golden/${MACHINE}/sentient-image.${ext}"
  if mc stat "$SRC" >/dev/null 2>&1; then
    mc cp "$SRC" "base-out/sentient-image-${MACHINE}.${ext}"
    mc cp "${SRC}.p7s" "base-out/sentient-image-${MACHINE}.${ext}.p7s" 2>/dev/null \
      || echo "::warning::no .p7s sidecar for $SRC in MinIO golden"
    FOUND=1
    break
  fi
done
[ "$FOUND" = 1 ] || { echo "::error::no pre-signed base in MinIO at minio/cache/golden/${MACHINE}/sentient-image.wic(.bz2) — run image.yml (workflow_dispatch) once to build + stage the shipping base, then re-run this release"; exit 1; }
for pem in signing-cert.pem ca.pem; do
  mc cp "minio/cache/golden/${MACHINE}/${pem}" "base-out/${pem}" 2>/dev/null || true
done
echo "::notice::pulled pre-signed base for ${MACHINE} from MinIO golden store"
ls -lh base-out

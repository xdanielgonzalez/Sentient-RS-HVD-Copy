#!/usr/bin/env bash
# .ci/scripts/release/stage-sysext-artifact.sh — collect the built app-sysext .raw into dist/.
#
# TODO(verify-at-build): confirm the actual BitBake output filename for the sysext squashfs —
# the frozen interface contract says "emits a squashfs `.raw` under the deploy dir (glob
# tolerantly for `sentient-app-sysext-<machine>*.squashfs` / `*.sysext*`)". Globbed tolerantly
# below per that guidance; narrow the glob once a real build tree confirms the exact name
# (typically build/tmp/deploy/images/<machine>/sentient-app-sysext-<machine>[-<datetime>].raw or
# a stable-symlinked variant, mirroring how sentient-image's .wic is named).
#
# Env contract: MACHINE. Reads yocto/build/tmp/deploy/images/$MACHINE, writes dist/.
set -euo pipefail

DEPLOY="yocto/build/tmp/deploy/images/${MACHINE}"
mkdir -p dist
SLUG="${MACHINE}"
shopt -s nullglob
copied=0
for f in "$DEPLOY"/sentient-app-sysext-*.raw "$DEPLOY"/sentient-app-sysext-*.squashfs "$DEPLOY"/*sysext*.raw "$DEPLOY"/*sysext*; do
  [ -f "$f" ] || continue
  case "$f" in *.spdx.json|*.manifest) continue ;; esac
  cp -L "$f" "dist/sentient-app-${SLUG}.raw"
  copied=1
  break
done
[ "$copied" = 1 ] || { echo "::error::no sentient-app-sysext .raw/.squashfs artifact found in $DEPLOY — check the sysext recipe's deploy output name"; ls -la "$DEPLOY" 2>/dev/null || true; exit 1; }
( cd dist && sha256sum "sentient-app-${SLUG}.raw" > "sentient-app-${SLUG}.sha256" )
ls -lh dist

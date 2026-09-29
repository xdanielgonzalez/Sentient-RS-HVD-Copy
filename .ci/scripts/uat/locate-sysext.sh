#!/usr/bin/env bash
# .ci/scripts/uat/locate-sysext.sh — find the emitted app-sysext squashfs and export SYSEXT_RAW.
#
# TODO(verify-at-build): confirm the exact emitted filename — glob tolerantly since the precise
# IMAGE_FSTYPES suffix for a squashfs sysext image is owned by the sentient-app-sysext recipe
# (sibling workstream), not this script.
#
# Env contract: MACHINE, KAS_WORK_DIR, GITHUB_ENV (exports SYSEXT_RAW for later steps).
set -euo pipefail

D="$KAS_WORK_DIR/build/tmp/deploy/images/${MACHINE}"
echo "=== deploy images dir ==="; ls -la "$D" || true
# Match ONLY the squashfs image (IMAGE_FSTYPES=squashfs). A `*.sysext*` glob also matches
# the create-spdx SBOM `sentient-app-sysext-<machine>.sysext.spdx.json`, and injecting THAT
# JSON as the extension made systemd-sysext fail with "Failed to read metadata ... No such
# file or directory" (it can't mount a JSON as a filesystem). `.squashfs` is precise.
RAW="$(find "$D" -maxdepth 1 -iname 'sentient-app-sysext-*.squashfs' 2>/dev/null | sort | head -1)"
test -n "$RAW" || { echo "::error::no sentient-app-sysext output found under $D — check the recipe's IMAGE_FSTYPES / TODO above"; exit 1; }
echo "SYSEXT_RAW=$RAW" >> "$GITHUB_ENV"
echo "::notice::built app sysext: $RAW ($(du -h "$RAW" | cut -f1))"

# DIAGNOSTIC (bounded debugging): confirm the extension-release NAME + contents from the
# PRE-squashfs rootfs (no unsquashfs needed — it isn't in the builder image). This
# disambiguates a bad extension-release.<name> from the loop/mount cause (missing
# CONFIG_BLK_DEV_LOOP, which sysext-fs.cfg now builds in) behind the boot error
# "systemd-sysext: Failed to read metadata for image sentient-app: No such file or directory".
echo "::group::sysext extension-release (from the pre-squashfs rootfs)"
erd="$KAS_WORK_DIR"/build/tmp/work/*/sentient-app-sysext/*/rootfs/usr/lib/extension-release.d
# shellcheck disable=SC2086 # $erd is a deliberate glob over the bitbake work dir
ls -la $erd 2>/dev/null || echo "(extension-release.d not found in the sysext rootfs)"
echo "--- extension-release contents ---"
# shellcheck disable=SC2086
cat $erd/extension-release.* 2>/dev/null || echo "(no extension-release.* file)"
echo "::endgroup::"

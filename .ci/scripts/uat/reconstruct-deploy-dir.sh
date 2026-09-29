#!/usr/bin/env bash
# .ci/scripts/uat/reconstruct-deploy-dir.sh — rebuild the deploy dir + merge the app sysext in.
#
# Reconstruct the deploy-dir layout the golden was built with (SAME abs path, so the pre-release's
# baked qemuboot.conf paths stay valid) and MERGE IN the fresh app sysext at the on-device deploy
# location, /var/lib/extensions/sentient-app.raw — a writable data-partition path per the frozen
# contract, NOT /usr. The tarball is the portable image-out bundle (rootfs.ext4 / kernel /
# qemuboot.conf / image.manifest, generic names) — rename back to the deploy convention so
# `runqemu qemuarm64` (deploy-dir mode) finds it, exactly as _yocto.yml's boot-smoke does. The
# write is offline via debugfs (e2fsprogs): no mount, no loop, no root-in-guest — same tooling as
# the old rootfs-injection approach, just writing to a legitimate data path instead of mutating
# /usr. systemd-sysext.service (enabled in the base) merges it into /usr at boot, before
# basic.target — this is the actual production mechanism, not an approximation of it.
#
# Env contract: MACHINE, KAS_WORK_DIR, GITHUB_WORKSPACE, SYSEXT_RAW (from locate-sysext.sh).
# Expects dl/sentient-image.tar.gz from pull-golden-base.sh.
set -euo pipefail
# shellcheck source=debugfs-lib.sh
. "$GITHUB_WORKSPACE/.ci/scripts/uat/debugfs-lib.sh"

D="$KAS_WORK_DIR/build/tmp/deploy/images/${MACHINE}"
mkdir -p "$D" /tmp/imgout
tar -xzf dl/*.tar.gz -C /tmp/imgout
echo "=== extracted image-out ==="; ls -la /tmp/imgout
test -f /tmp/imgout/rootfs.ext4    || { echo "::error::golden bundle has no rootfs.ext4"; exit 1; }
test -f /tmp/imgout/qemuboot.conf  || { echo "::error::golden bundle has no qemuboot.conf"; exit 1; }
test -f /tmp/imgout/kernel         || { echo "::error::golden bundle has no kernel"; exit 1; }
cp /tmp/imgout/rootfs.ext4    "$D/sentient-image-${MACHINE}.rootfs.ext4"
cp /tmp/imgout/qemuboot.conf  "$D/sentient-image-${MACHINE}.qemuboot.conf"
cp /tmp/imgout/kernel         "$D/Image"   # qemuarm64 kernel is Image
cp /tmp/imgout/image.manifest "$D/sentient-image-${MACHINE}.rootfs.manifest" 2>/dev/null || true
ROOTFS="$D/sentient-image-${MACHINE}.rootfs.ext4"
RAW="$SYSEXT_RAW"; test -f "$RAW"
# Ensure /var/lib/extensions exists in the golden base (mkdir is a no-op error if already
# present — tolerate that). TODO(verify-at-build): confirm debugfs `mkdir` is the right verb
# here (vs `mkdir` needing the parent /var/lib to already exist, which it should on a
# systemd-sysext-enabled base) and that no SELinux/xattr labeling step is also required.
dfs_mkdir "$ROOTFS" /var/lib/extensions
# TODO(verify-at-build): confirm this debugfs `write` form places the file directly under
# /var/lib/extensions (some debugfs builds require the parent dir to be the current debugfs
# cwd via `cd` first, rather than accepting a full nested destination path on `write`).
dfs_write "$ROOTFS" "$RAW" /var/lib/extensions/sentient-app.raw
echo "=== written sysext inode ==="
debugfs -R "stat /var/lib/extensions/sentient-app.raw" "$ROOTFS" 2>/dev/null | head -20 || true
chown -R builder:builder "$KAS_WORK_DIR" "$GITHUB_WORKSPACE/distro" /opt/yocto-cache 2>/dev/null || true

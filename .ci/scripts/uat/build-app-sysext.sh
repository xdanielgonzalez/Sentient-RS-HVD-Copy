#!/usr/bin/env bash
# .ci/scripts/uat/build-app-sysext.sh — stage the freshly-built bin-rpi + kas-build the app sysext.
#
# Stage the freshly-built binary where the sentient-agent-bin recipe expects it (same staging
# step the real build/release pipeline uses — see the FROZEN interface contract), then build
# ONLY the app sysext image for $MACHINE. This does NOT rebuild the base sentient-image; it
# produces the small squashfs .raw carrying /usr/bin/bin-rpi + the extension-release metadata.
#
# TODO(verify-at-build): confirm the exact staging path is
# distro/meta-sentient/recipes-sentient/sentient-agent/files/bin-rpi (per the frozen contract) —
# this script does not own the recipe and cannot see its final FILESPATH layout.
#
# Env contract (set by the calling workflow step):
#   MACHINE           — kas machine (qemuarm64 for UAT)
#   KAS_WORK_DIR      — kas work dir (<workspace>/yocto)
#   GITHUB_WORKSPACE  — repo checkout root
set -euo pipefail

NEW="$PWD/newbin/bin-rpi"; test -f "$NEW"; chmod +x "$NEW"
STAGE_DIR="distro/meta-sentient/recipes-sentient/sentient-agent/files"
mkdir -p "$STAGE_DIR"
cp "$NEW" "$STAGE_DIR/bin-rpi"
echo "::notice::staged fresh bin-rpi into $STAGE_DIR/bin-rpi for sentient-agent-bin"

# kas chdir()s into KAS_WORK_DIR immediately — it must EXIST first (kas does not create it),
# else `os.chdir` throws FileNotFoundError. Shared kas harness (mkdir + chown + runuser drop) —
# mirrors _yocto.yml/production.yml.
# PROGRESS-ONLY REPORTING: pipe the BitBake build through the shared log filter (throttled
# progress heartbeat + all warnings/errors/summaries, minus the per-task NOTE flood). The
# `set -o pipefail` above keeps BitBake's real exit code authoritative across the pipe.
KAS_MACHINE="$MACHINE" CHOWN_PATHS="$GITHUB_WORKSPACE /opt/yocto-cache" \
  "$GITHUB_WORKSPACE/.ci/scripts/run-kas.sh" build "distro/kas/sentient.yml" --target sentient-app-sysext \
  | python3 "$GITHUB_WORKSPACE/.ci/scripts/bitbake-log-filter.py"

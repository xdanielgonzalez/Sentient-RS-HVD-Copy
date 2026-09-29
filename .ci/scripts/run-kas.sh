#!/usr/bin/env bash
# .ci/scripts/run-kas.sh — shared kas privilege-drop + workdir setup harness.
#
# Extracted from the identical boilerplate at the top of every "kas build"/"kas shell" step in
# _yocto.yml, production.yml's `sysext` job, and uat.yml's sysext-build step: the ARC runner
# container defaults to root (needed so actions/checkout's root-owned
# /__w/_temp/_runner_file_commands/ is writable), but BitBake refuses to run as root — so kas is
# always invoked as the non-root `builder` user via `runuser`, after ensuring $KAS_WORK_DIR exists
# (kas chdir()s into it immediately; it does not create it) and is builder-owned (along with any
# other paths the build needs to write, e.g. the checked-out distro/ tree or a generated kas
# parallelism fragment).
#
# Usage:
#   KAS_WORK_DIR=<dir> CHOWN_PATHS="<space-separated paths>" [KAS_MACHINE=<machine>] \
#     [DL_DIR=<dir>] [SSTATE_DIR=<dir>] \
#     .ci/scripts/run-kas.sh <kas subcommand + args...>
#
# e.g.:
#   KAS_WORK_DIR="$GITHUB_WORKSPACE/yocto" CHOWN_PATHS="$GITHUB_WORKSPACE" \
#     .ci/scripts/run-kas.sh build "distro/kas/sentient.yml:$PARCONF" --target sentient-app-sysext
#
# Behavior preserved exactly: mkdir -p "$KAS_WORK_DIR"; chown -R builder:builder over CHOWN_PATHS
# (best-effort, `|| true` — matches every original site); then
#   runuser -u builder -- env HOME=/home/builder [KAS_MACHINE=...] KAS_WORK_DIR=... \
#     [DL_DIR=...] [SSTATE_DIR=...] kas "$@"
# KAS_MACHINE/DL_DIR/SSTATE_DIR are only passed through to the runuser env when set (some call
# sites — production.yml's sysext job — never set KAS_MACHINE at all).
set -euo pipefail

: "${KAS_WORK_DIR:?set KAS_WORK_DIR}"
CHOWN_PATHS="${CHOWN_PATHS:-$KAS_WORK_DIR}"

mkdir -p "$KAS_WORK_DIR"
# shellcheck disable=SC2086  # CHOWN_PATHS is a deliberately word-split space-separated path list.
chown -R builder:builder $CHOWN_PATHS 2>/dev/null || true

ENV_ARGS=(HOME=/home/builder KAS_WORK_DIR="$KAS_WORK_DIR")
[ -n "${KAS_MACHINE:-}" ]  && ENV_ARGS+=(KAS_MACHINE="$KAS_MACHINE")
[ -n "${DL_DIR:-}" ]       && ENV_ARGS+=(DL_DIR="$DL_DIR")
[ -n "${SSTATE_DIR:-}" ]   && ENV_ARGS+=(SSTATE_DIR="$SSTATE_DIR")

runuser -u builder -- env "${ENV_ARGS[@]}" kas "$@"

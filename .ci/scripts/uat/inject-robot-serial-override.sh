#!/usr/bin/env bash
# .ci/scripts/uat/inject-robot-serial-override.sh — ROBOT-BOOT-ONLY serial override.
#
# Runs AFTER preflight has passed (and torn down its snapshot boot), so it re-points [serial]
# at the real 2nd UART + widens the systemd cgroup device filter to allow it, applying ONLY to
# the Robot boot (which reads the PERSISTENT rootfs and DOES start the Seplos sim to feed
# ttyAMA1). See the header comment in inject-e2e-config.sh for the full regression story
# (UAT run 1: pointing preflight at an unfed ttyAMA1 -> read-timeout -> backoff -> watchdog
# kill). Skipped when the Robot suites don't exist yet (same guard as the Robot/sim steps) so a
# suite-less run's preflight-only gate keeps the shipped-ttyAMA0 clean-degraded config untouched.
#
# Two writes into the reconstructed rootfs (same debugfs mechanism as every other injection):
#   1. /etc/sentient/sentient.toml — REWRITTEN with [serial] port=/dev/ttyAMA1 (broker bind
#      stays 0.0.0.0). This overwrites the broker-bind-only variant inject-e2e-config.sh wrote.
#   2. /etc/systemd/system/sentient-agent.service.d/e2e-override.conf — a drop-in ADDING
#      `DeviceAllow=/dev/ttyAMA1 rw` (DeviceAllow accumulates; no need to reset the shipped
#      DeviceAllow=/dev/ttyUSB0). The shipped sentient-agent.service sets DevicePolicy=closed +
#      DeviceAllow=/dev/ttyUSB0 rw ONLY — its own inline TODO flags the ttyUSB0-vs-ttyAMA0
#      mismatch. Without this drop-in the cgroup device filter blocks bin-rpi's open() of
#      ttyAMA1 even with the correct [serial].port, and the agent runs DEGRADED with no frames.
#
# TODO(verify-in-ci): /dev/ttyAMA1 CONFIRMED by UAT run 1's E2E-DIAG output (9040000.pl011:
# ttyAMA1, crw-rw---- root dialout 204,65). If a future poky bump renumbers it, correct
# E2E_SERIAL_GUEST_DEV in the calling workflow (the DIAG unit re-dumps it every run).
#
# TODO(verify-in-ci): the reconnect() self-EBUSY (tokio-serial TIOCEXCL on bin-rpi's own still-
# open fd — see impl_host_serial.rs::open) means that IF the sim ever disconnects mid-Robot-run,
# bin-rpi can't cleanly reopen ttyAMA1 until the old fd is dropped. Not a blocker for the happy
# path (sim stays connected for the whole run), but if the Robot suite later drives sim
# disconnect/reconnect scenarios, that self-EBUSY is the thing to fix in bin-rpi (drop the old
# transport BEFORE reopening in Platform::reconnect) — flagged, not worked around in the harness.
#
# Env contract: MACHINE, KAS_WORK_DIR, GITHUB_WORKSPACE, E2E_SERIAL_GUEST_DEV.
set -euo pipefail
# shellcheck source=debugfs-lib.sh
. "$GITHUB_WORKSPACE/.ci/scripts/uat/debugfs-lib.sh"

if ! ls distro/qemu/*.robot >/dev/null 2>&1; then
  echo "::notice::serial override skipped — no distro/qemu/*.robot suites yet (#156); preflight-only run keeps the clean-degraded ttyAMA0 config."
  exit 0
fi
D="$KAS_WORK_DIR/build/tmp/deploy/images/${MACHINE}"
ROOTFS="$D/sentient-image-${MACHINE}.rootfs.ext4"
test -f "$ROOTFS" || { echo "::error::reconstructed rootfs not found at $ROOTFS"; exit 1; }

cat > /tmp/e2e-sentient-robot.toml <<EOF
[device]
id = "METEOR-001"

[serial]
kind = "rs485"
port = "${E2E_SERIAL_GUEST_DEV}"
baud = 19200
parity = "none"
stop_bits = 1
read_timeout_ms = 1000

[poll]
dip_addresses = [0]
interval_ms = 1000
fault_every = 10

[broker]
# WIDENED FOR THIS E2E ONLY (isolated QEMU slirp segment) — never a real-deployment template.
host = "0.0.0.0"
port = 1883

[publish]
prefix = "tms"
EOF

cat > /tmp/e2e-override.conf <<EOF
[Service]
DeviceAllow=${E2E_SERIAL_GUEST_DEV} rw
EOF

echo "=== Robot-boot sentient.toml (serial -> ${E2E_SERIAL_GUEST_DEV}) ==="; cat /tmp/e2e-sentient-robot.toml
echo "=== Robot-boot systemd drop-in ==="; cat /tmp/e2e-override.conf

dfs_write "$ROOTFS" /tmp/e2e-sentient-robot.toml /etc/sentient/sentient.toml

# /etc/systemd/system/sentient-agent.service.d/ does not exist on the golden base — create
# it (mkdir needs the parent /etc/systemd/system, which exists on any systemd base).
dfs_mkdir "$ROOTFS" /etc/systemd/system/sentient-agent.service.d
dfs_write "$ROOTFS" /tmp/e2e-override.conf /etc/systemd/system/sentient-agent.service.d/e2e-override.conf

echo "=== verify: sentient.toml on rootfs (should now show ttyAMA1) ==="
dfs_cat "$ROOTFS" /etc/sentient/sentient.toml || echo "(read-back failed)"
echo "=== verify: e2e-override.conf on rootfs ==="
dfs_cat "$ROOTFS" /etc/systemd/system/sentient-agent.service.d/e2e-override.conf || echo "(read-back failed)"
echo "::notice::Robot-boot serial override applied — [serial] port=${E2E_SERIAL_GUEST_DEV} + DeviceAllow. This rootfs now boots the Robot e2e (which feeds ttyAMA1 via the sim)."

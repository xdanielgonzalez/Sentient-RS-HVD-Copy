#!/usr/bin/env bash
# .ci/scripts/uat/inject-e2e-config.sh — E2E CONFIG, BROKER-BIND-ONLY (shared by BOTH boots)
# + the mosquitto no-listener fix + the E2E-DIAG probe unit.
#
# ==== WHY THIS IS SPLIT FROM THE SERIAL OVERRIDE (regression fix, UAT run 1) ==================
# UAT run 1 confirmed the sysext merge + config read work (bin-rpi logged "broker 0.0.0.0:1883")
# and that the 2nd UART is /dev/ttyAMA1 (crw-rw---- root dialout 204,65; the only getty is on
# ttyAMA0). But PREFLIGHT then FAILED: the earlier version of this step wrote [serial]
# port=/dev/ttyAMA1 into the SHARED golden rootfs, so it applied to the PREFLIGHT boot too — and
# preflight starts NO Seplos sim, so ttyAMA1 is a real, openable, but UNFED UART. bin-rpi opened
# it, every read timed out, the dead-cycle backoff climbed toward 30 s, and a single loop
# iteration's backoff sleep eventually exceeded the unit's WatchdogSec=30 before on_cycle() could
# feed the watchdog again -> "sentient-agent.service: Failed with result 'watchdog'". (The
# reconnect() path compounds it: tokio-serial's open_native_async sets TIOCEXCL, and bin-rpi's
# own still-open exclusive fd makes the reopen attempt fail EBUSY — the "Device or resource busy"
# the run 1 log showed is bin-rpi self-blocking its own reconnect, NOT an external holder. See
# crates/lib-io/src/host/impl_host_serial.rs::open + crates/impl-app/src/lib.rs's dead-cycle
# backoff.) Before this wiring, preflight PASSED because the shipped [serial] port=/dev/ttyAMA0 is
# the CONSOLE UART (serial-getty holds it) -> bin-rpi's open fails at build -> HostSerial
# ::disconnected() -> the clean, non-fatal DEGRADED path (impl_host_platform.rs::build), watchdog
# still fed (liveness, not telemetry health — impl_host_platform.rs::on_cycle).
#
# THE FIX (per coordinator's lean, least-fragile): serial config + 2nd-UART + sim are a
# ROBOT-BOOT-ONLY concern; broker-bind is the only thing safe to share. So:
#   - THIS script (before preflight) injects ONLY the broker-bind widening into the shared
#     sentient.toml, and KEEPS [serial] port=/dev/ttyAMA0 (the shipped console default) so the
#     preflight boot degrades cleanly exactly as it did pre-wiring. Preflight validates ONLY the
#     sysext merge + that the agent runs — it does NOT require a working serial.
#   - A SEPARATE script AFTER preflight (inject-robot-serial-override.sh) re-writes sentient.toml
#     with [serial] port=/dev/ttyAMA1 + adds the DeviceAllow drop-in. That applies ONLY to the
#     Robot boot, which DOES start the sim to feed ttyAMA1. This works because the Robot boot
#     reads the PERSISTENT rootfs (YoctoQemuKeywords' runqemu has NO `snapshot`), while preflight
#     boots the SAME file with `snapshot` (COW) and has already run and been torn down by the
#     time the post-preflight injection lands.
#
# broker-bind rationale: [broker] host="0.0.0.0" — slirp hostfwd forwards HOST:port ->
# GUEST-IP:port and cannot reach a guest-loopback-only bind (crates/bin/rpi/src/config/
# impl_broker_toml.rs Default = 127.0.0.1). Widening ships auth:None (impl_config.rs::
# broker_config doc comment — no CONNECT authorizer backend in lib-security yet); SAFE here only
# because this is an isolated QEMU slirp segment reachable solely from the process that started
# runqemu. NEVER copy this into a real deployment's sentient.toml. Harmless to the preflight boot
# (it just makes the broker listen more broadly on an isolated net; preflight doesn't connect).
#
# Env contract: MACHINE, KAS_WORK_DIR, GITHUB_WORKSPACE.
set -euo pipefail
# shellcheck source=debugfs-lib.sh
. "$GITHUB_WORKSPACE/.ci/scripts/uat/debugfs-lib.sh"

D="$KAS_WORK_DIR/build/tmp/deploy/images/${MACHINE}"
ROOTFS="$D/sentient-image-${MACHINE}.rootfs.ext4"
test -f "$ROOTFS" || { echo "::error::reconstructed rootfs not found at $ROOTFS"; exit 1; }

# NOTE: [serial] port stays /dev/ttyAMA0 (the shipped console default) HERE — see the header
# comment: preflight must degrade cleanly on serial, not open a real-but-unfed UART. The
# Robot-boot-only script re-points it at ttyAMA1.
cat > /tmp/e2e-sentient.toml <<EOF
[device]
id = "METEOR-001"

[serial]
kind = "rs485"
port = "/dev/ttyAMA0"
baud = 19200
parity = "none"
stop_bits = 1
read_timeout_ms = 1000

[poll]
dip_addresses = [0]
interval_ms = 1000
fault_every = 10

[broker]
# WIDENED FOR THIS E2E ONLY (isolated QEMU slirp segment) — never a template for a real
# deployment's sentient.toml (ships auth:None; see header comment above).
host = "0.0.0.0"
port = 1883

[publish]
prefix = "tms"
EOF

echo "=== injected sentient.toml (broker-bind only; serial stays ttyAMA0 for preflight) ==="
cat /tmp/e2e-sentient.toml

dfs_write "$ROOTFS" /tmp/e2e-sentient.toml /etc/sentient/sentient.toml

echo "=== verify: sentient.toml on rootfs ==="
dfs_cat "$ROOTFS" /etc/sentient/sentient.toml || echo "(read-back failed)"
echo "::notice::injected e2e broker bind (0.0.0.0), serial kept at /dev/ttyAMA0 for a clean preflight-degraded boot"

# MOSQUITTO NO-LISTENER FIX (fast-verify of the durable recipe change on this run's
# already-built golden, WITHOUT waiting for a full image.yml golden rebuild). TWO stacked
# root causes, both proven from qemu_console.log:
#   (A) UAT 29430960362 + 29433182716: mosquitto opens its DEFAULT localhost:1883 listener
#       ("Starting in local only mode" -> "Opening ipv4 listen socket on port 1883"), racing
#       + stealing 1883 from bin-rpi's embedded rumqttd device broker (rumqttd dies with
#       EADDRINUSE) — so NO external TCP listener exists and every Robot test fails at suite
#       setup with "Connection timeout after 300s".
#   (B) UAT 29433182716 revealed WHY the sentient.conf listener fix alone didn't help: the
#       stock meta-networking mosquitto_2.1.2 recipe installs upstream's mosquitto.conf with
#       `include_dir` COMMENTED OUT, so /etc/mosquitto/conf.d/sentient.conf is NEVER read —
#       mosquitto ignored both our bridge and our listener directive (no `device-broker`
#       bridge attempt ever appeared in the console). The whole mosquitto bridge had in fact
#       never loaded on any boot.
# So we inject BOTH: (1) append `include_dir /etc/mosquitto/conf.d` to the baked
# mosquitto.conf so mosquitto actually loads conf.d, and (2) the repo's now-fixed
# sentient.conf verbatim (unix-socket `listener 0 /run/mosquitto/mosquitto.sock` -> suppresses
# the default-1883 listener, ZERO TCP ports) + a RuntimeDirectory=mosquitto drop-in so
# /run/mosquitto exists for that socket. Shared by BOTH boots + strictly preflight-safe: it
# only frees 1883 for rumqttd, never opens a new port. Becomes a redundant no-op once the next
# golden ships the fixed recipe (same content, idempotent include guard).
#
# (B) fix: append the conf.d include to the baked mosquitto.conf (dump -> append -> write
# back; debugfs has no in-place append). Guard so a re-run / already-fixed golden doesn't
# double-include (which would parse sentient.conf twice -> duplicate listener -> startup err).
debugfs -R "dump /etc/mosquitto/mosquitto.conf /tmp/mosquitto.conf.baked" "$ROOTFS" 2>/dev/null || : > /tmp/mosquitto.conf.baked
cp /tmp/mosquitto.conf.baked /tmp/mosquitto.conf.new
if ! grep -qE "^[[:space:]]*include_dir[[:space:]]+/etc/mosquitto/conf.d[[:space:]]*$" /tmp/mosquitto.conf.new; then
  printf '\n# Sentient e2e: load conf.d (device-broker bridge + unix-socket listener)\ninclude_dir /etc/mosquitto/conf.d\n' >> /tmp/mosquitto.conf.new
fi
# Second include_dir (cloud-relay dir) — BOTH must be in the MAIN config; mosquitto 2.x
# forbids a nested include_dir inside conf.d/sentient.conf (that crash-looped mosquitto on
# UAT 29434470958, killing the relay). Empty in the e2e (no cloud) -> harmless no-op.
if ! grep -qE "^[[:space:]]*include_dir[[:space:]]+/etc/sentient/mosquitto.d[[:space:]]*$" /tmp/mosquitto.conf.new; then
  printf '\n# Sentient e2e: load the rendered cloud-relay bridge dir\ninclude_dir /etc/sentient/mosquitto.d\n' >> /tmp/mosquitto.conf.new
fi
dfs_write "$ROOTFS" /tmp/mosquitto.conf.new /etc/mosquitto/mosquitto.conf
echo "=== verify: mosquitto.conf now includes conf.d ==="
dfs_cat "$ROOTFS" /etc/mosquitto/mosquitto.conf | grep -iE "include_dir" || echo "(no include_dir — FIX FAILED)"

# (A) fix: the repo's now-fixed sentient.conf (unix-socket listener, no TCP port).
MOSQ_CONF="distro/meta-sentient/recipes-connectivity/mosquitto/files/sentient.conf"
test -f "$MOSQ_CONF" || { echo "::error::fixed mosquitto sentient.conf not found at $MOSQ_CONF"; exit 1; }
dfs_write "$ROOTFS" "$MOSQ_CONF" /etc/mosquitto/conf.d/sentient.conf
cat > /tmp/e2e-mosq-runtimedir.conf <<'EOF'
[Service]
RuntimeDirectory=mosquitto
EOF
dfs_mkdir "$ROOTFS" /etc/systemd/system/mosquitto.service.d
dfs_write "$ROOTFS" /tmp/e2e-mosq-runtimedir.conf /etc/systemd/system/mosquitto.service.d/e2e-runtimedir.conf
echo "=== verify: mosquitto sentient.conf on rootfs (should show 'listener 0 /run/mosquitto/mosquitto.sock', NO tcp 1883) ==="
dfs_cat "$ROOTFS" /etc/mosquitto/conf.d/sentient.conf | grep -iE "listener|allow_anonymous" || echo "(read-back failed)"
echo "::notice::injected mosquitto no-listener fix (unix socket) — frees 1883 for bin-rpi's rumqttd device broker"

# EMPIRICAL DIAGNOSTIC (bounded debugging, not a guess-and-rebuild loop): a tiny oneshot
# unit that dumps exactly what the guest exposes for tty/serial + WHO (if anyone) holds the
# 2nd UART busy + confirms the sysext merge, logged via systemd-cat so it lands in the
# journal — which the preflight boot already forwards to the console it scrapes
# (bootparams='systemd.journald.forward_to_console=1'). Run 1 already used this to confirm
# ttyAMA1; run 2 EXTENDS it with fuser/lsof on ttyAMA1 to prove the run-1 EBUSY was
# bin-rpi's OWN reconnect (self-TIOCEXCL) and not an external holder, AND dumps the guest's
# own view of the qemu-provided serial devices. WantedBy=multi-user.target so it runs on
# every e2e boot regardless of sentient-agent.service's own success/failure.
cat > /tmp/sentient-diag.service <<'EOF'
[Unit]
Description=E2E DIAGNOSTIC — dump guest tty/serial nodes + who-holds-ttyAMA1 + sysext status (test-harness only, not shipped)
After=systemd-sysext.service
Wants=systemd-sysext.service

[Service]
Type=oneshot
ExecStart=/bin/sh -c 'echo "=== E2E-DIAG: /dev/tty* + /dev/serial* ==="; ls -l /dev/tty* /dev/serial* 2>&1; echo "=== E2E-DIAG: dmesg uart/pl011/serial ==="; dmesg 2>&1 | grep -iE "ttyAMA|uart|pl011|serial" 2>&1; echo "=== E2E-DIAG: who-holds /dev/ttyAMA1 (fuser/lsof) ==="; fuser -v /dev/ttyAMA1 2>&1 || echo "(fuser: no holder or fuser absent)"; lsof /dev/ttyAMA1 2>&1 || echo "(lsof: no holder or lsof absent)"; echo "=== E2E-DIAG: serial-getty units ==="; systemctl list-units --type=service --all 2>&1 | grep -iE "getty|serial" 2>&1; echo "=== E2E-DIAG: systemd-sysext status ==="; systemd-sysext status 2>&1; echo "=== E2E-DIAG: end ==="' | systemd-cat -t e2e-diag
RemainAfterExit=yes

[Install]
WantedBy=multi-user.target
EOF

dfs_write "$ROOTFS" /tmp/sentient-diag.service /etc/systemd/system/sentient-diag.service
# Enable it the same offline way #218/#219 enabled systemd-sysext.service (SYSTEMD_AUTO_ENABLE
# misses hand-injected units — a direct wants-symlink is required).
dfs_mkdir "$ROOTFS" /etc/systemd/system/multi-user.target.wants
debugfs -w -R "symlink /etc/systemd/system/multi-user.target.wants/sentient-diag.service /etc/systemd/system/sentient-diag.service" "$ROOTFS" 2>/dev/null || true
echo "=== verify: sentient-diag.service enabled ==="
debugfs -R "ls -l /etc/systemd/system/multi-user.target.wants" "$ROOTFS" 2>/dev/null | grep -i diag || echo "(wants-symlink not confirmed — diagnostic may not run; check journal grep for 'E2E-DIAG' regardless, systemd may still pick it up via other means)"
echo "::notice::injected E2E-DIAG oneshot unit — its output ('E2E-DIAG:' lines) will appear in the Preflight console log"

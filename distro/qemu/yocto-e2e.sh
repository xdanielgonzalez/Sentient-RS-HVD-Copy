#!/usr/bin/env bash
# yocto-e2e.sh — functional QEMU e2e for the Yocto `sentient-image` (Task #156 / Story #159).
#
# WHY THIS EXISTS / WHY IT'S NEW: this is the FIRST end-to-end that can drive the Pi shipping image
# through a real network stack. The old Zephyr-on-Pi plan had no emulated NIC for the Pi (CLAUDE.md's
# "RPi networking is BLOCKED" doctrine — no L2 driver in Zephyr for the Pi 5). Running Linux on the
# Pi dissolves that blocker: `runqemu`'s `qemuarm64` proxy machine has a real, emulatable NIC (slirp
# user-net) and a real UART, so — for the first time — we can boot the actual shipping image under
# QEMU, feed it simulated Seplos BMS frames on a serial port, and prove it publishes real TMS
# payloads over MQTT, entirely without hardware.
#
# MODELED ON the retired MCU harness's qemu-e2e.sh (same warm-container + persistent-cache
# philosophy; that harness was deleted with the Embassy pivot, see
# docs/decisions/0010-mcu-runtime-embassy.md), but genuinely NOT that harness: no `west`/Zephyr/CMake build here at
# all — the "build" step is `kas build ... --target sentient-image` (BitBake/Yocto), and the "run"
# step is `runqemu` (not a bare `qemu-system-*` invocation) because runqemu is the thing that knows
# how to locate the MACHINE's kernel/rootfs/wic artifacts that a `kas build` just produced. Kept
# CARGO-CULT-FREE: this script is deliberately its own thing, not a copy-paste of qemu-e2e.sh's
# west-specific plumbing. Where the SAME problem recurs (warm container, persistent cache dir,
# distinct TAP/socket names so concurrent runs never collide, a `run [N]` loop convention so
# `just yocto-e2e-run N` mirrors `just qemu-e2e-run N`), the SAME solution is reused on purpose.
#
# WHAT THIS DOES NOT OWN: the meta-sentient Yocto layer (distro/meta-sentient/**), the kas manifests
# (distro/kas/*.yml), the justfile `yocto-*` recipes, and release.yml's `yocto` job are ALL owned by
# other workstreams — this script only CALLS `kas build` (same command those recipes/that job use)
# and `runqemu` on their output. It never edits any of those files.
#
# ---------------------------------------------------------------------------------------------
# HARNESS DESIGN
# ---------------------------------------------------------------------------------------------
#
#   1. A HOST-PERSISTENT build cache dir (default /tmp/yocto-build-cache) bind-mounted into the
#      container as /build. Yocto's DL_DIR (downloads) + SSTATE_DIR (shared-state) live here and
#      survive across invocations — mandatory for a tractable Yocto build time (see
#      .ci/yocto.Dockerfile's own comment: sstate reuse is the difference between hours and minutes).
#      This is DISTINCT from qemu-e2e.sh's /tmp/qemu-build-cache (different dir, different content:
#      Yocto sstate/downloads vs. a west/Zephyr workspace) so the two harnesses never collide or
#      cross-pollute caches if run concurrently on the same host/CI node.
#   2. A WARM, long-lived container (`docker run -d ... sleep infinity`, named yocto-e2e — distinct
#      from qemu-e2e.sh's `qemu-e2e` container name) started once and reused via `docker exec`, so
#      apt/pip deps (the paho-mqtt venv this script's external MQTT client needs — kas/BitBake
#      itself is already baked into sentient-yocto-builder, see .ci/yocto.Dockerfile) are installed
#      once, guarded by a sentinel, not on every invocation.
#   3. REPO SYNC, not a bind-mount of the working tree: kas/BitBake writes `build/`, `poky/`,
#      `meta-openembedded/`, etc. as SIBLINGS of wherever `kas build` is invoked from (kas's default
#      repo layout places fetched layers next to the invoking directory, matching how
#      distro/kas/sentient.yml's own header describes `kas build distro/kas/sentient.yml` being run
#      "from inside sentient-yocto-builder" — see justfile's yocto-build/yocto-image comments). To
#      keep those multi-GB clones and the BitBake tmp/ tree out of the actual working tree (and off
#      your `git status`), this script tar-syncs the repo into /build/ws/repo once (excluding
#      target/.git/build/zephyr/modules/tools/bootloader/.west — the same exclusion philosophy as
#      qemu-e2e.sh's cmd_sync, extended to also skip Yocto's OWN build/ dir so a stale local BitBake
#      tree never leaks into the synced copy) and re-syncs cheaply on every build.
#   4. TAP/socket names: this script uses TCP loopback sockets for both the serial-sim chardev and
#      the MQTT hostfwd (127.0.0.1, non-privileged ports in the 15xxx range, distinct from
#      qemu-e2e.sh's `zethc` TAP + 5001/1883 conventions) — chosen specifically so a concurrent
#      qemu-e2e.sh (MCU/Renode) run and this Yocto run never collide on the same CI/dev host.
#
# ---------------------------------------------------------------------------------------------
# NETWORK / SERIAL WIRING (see distro/qemu/README.md for the full rationale + citations)
# ---------------------------------------------------------------------------------------------
#
#   - MQTT (guest broker -> host paho client): slirp user-net hostfwd. `runqemu <machine> nographic
#     slirp serial qemuparams="-netdev user,id=net0,hostfwd=tcp::${YOCTO_E2E_MQTT_PORT}-:1883"`. The
#     guest's `sentient-agent` (bin-rpi) must bind its embedded broker to 0.0.0.0:1883, NOT the
#     shipping default of 127.0.0.1:1883 (see config note below) — otherwise slirp's hostfwd has
#     nothing reachable to forward to from outside the guest's own loopback.
#   - Serial (host SeplosSim -> guest UART): a host-side `-chardev socket,id=seplossim,path=...`
#     (or `,host=127.0.0.1,port=...` TCP form) attached to a second guest UART via `-serial
#     chardev:seplossim`, passed through runqemu's `qemuparams=`. SeplosSim then dials in as a
#     TCP/socket CLIENT (`seplos-tcp-sim --connect ...`), mirroring exactly how qemu-e2e.sh's sim
#     dials QEMU's own `-serial tcp:...,server,nowait` socket — same proven "sim connects early,
#     retries until QEMU's socket is listening" pattern, reused verbatim below.
#     # TODO(verify-at-build): confirm qemuarm64 exposes a SECOND UART node out of the box (the
#     # plan's own verified-facts flag this open) — inspect the pinned poky's
#     # meta/conf/machine/qemuarm64.conf + its linked qemuboot.conf for now many `-serial` slots
#     # runqemu wires by default (typically ONE console UART; a second may need an extra
#     # `-serial` qemuparams entry to just WORK if the guest's device tree / kernel already exposes
#     # ttyS1, or may need a DT overlay / kernel cmdline `earlycon`/`console=` tweak if it does not).
#     # If qemuarm64 truly has only one UART, the fallback is to multiplex: use the ONE console UART
#     # for both the boot console log AND the Seplos link, with bin-rpi's `[serial] port` pointed at
#     # that single node and runqemu's own console redirected to a second `-serial` (e.g.
#     # `-serial mon:stdio -serial chardev:seplossim` — many qemuarm64/virt-style boards DO expose at
#     # least two PL011/8250 UARTs; confirm the exact devicetree node name -> /dev/ttyAMA1 vs
#     # /dev/ttyS1 mapping at build time and update SERIAL_GUEST_DEV below).
#   - The e2e-only `sentient.toml` (rendered by this script, NOT the shipping default in
#     config/sentient.toml or distro/deploy/config.example.toml — those stay loopback-only and
#     hardware-serial-pathed on purpose) sets:
#       [serial] port = "${SERIAL_GUEST_DEV}"     # see TODO above — guest-side device node
#       [broker]  host = "0.0.0.0"                # widened ONLY for this isolated QEMU e2e network
#     Widening [broker].host beyond loopback ships today with `auth: None` (see
#     crates/bin/rpi/src/config/impl_config.rs::broker_config's own doc comment — no CONNECT
#     authorizer backend exists yet in lib-security). That is FINE and deliberate here: this e2e's
#     network is an isolated QEMU slirp segment reachable only from the CI runner/host process that
#     started it, not a real deployment — but it must NEVER be treated as a template for a real
#     device's [broker] section (those stay 127.0.0.1). Call this out loudly in any doc that copies
#     this config.
#
# ---------------------------------------------------------------------------------------------
# USAGE:
#   yocto-e2e.sh up                 # start (or reuse) the warm container
#   yocto-e2e.sh down               # stop + remove the warm container
#   yocto-e2e.sh build [MACHINE]    # kas build sentient-image for MACHINE (default qemuarm64)
#   yocto-e2e.sh run [N] [MACHINE]  # build, then run the functional e2e once (or N times)
#
# ENV overrides:
#   YOCTO_E2E_CACHE       host build-cache dir      (default /tmp/yocto-build-cache)
#   YOCTO_E2E_IMAGE       builder image             (default ghcr.io/high-velocity-decisions/sentient-yocto-builder:latest)
#   YOCTO_E2E_MACHINE     emulation-proxy MACHINE   (default qemuarm64 — ARM only, no x86)
#   YOCTO_E2E_MQTT_PORT   host-side forwarded MQTT port (default 18830, NOT 1883 — avoids colliding
#                         with any other local mosquitto/broker already bound to 1883 on the host)
#   YOCTO_E2E_SERIAL_PORT host-side TCP port the seplos-tcp-sim dials     (default 15001)
#
# TODO(verify-at-build): this entire script is authored GREENFIELD against Yocto/runqemu docs (see
# README.md's citation list) and has never been run — there is no local Yocto/runqemu/QEMU image to
# validate against in this environment (explicit constraint of this task). Every runqemu qemuparams
# flag, the guest UART device path, and the exact kas/BitBake artifact paths are flagged inline
# with TODO(verify-at-build) where they are not yet proven; treat a first CI run of this script as
# the actual bring-up, not a rubber-stamp — mirrors release.yml yocto job's own header comment.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CONTAINER_NAME="yocto-e2e"
CACHE_DIR="${YOCTO_E2E_CACHE:-/tmp/yocto-build-cache}"
IMAGE="${YOCTO_E2E_IMAGE:-ghcr.io/high-velocity-decisions/sentient-yocto-builder:latest}"
MACHINE="${YOCTO_E2E_MACHINE:-qemuarm64}"
MQTT_HOST_PORT="${YOCTO_E2E_MQTT_PORT:-18830}"
SERIAL_HOST_PORT="${YOCTO_E2E_SERIAL_PORT:-15001}"

# TODO(verify-at-build): the guest-side serial device the Seplos link rides on. /dev/ttyS1 is this
# script's working assumption (a second 8250-class UART, the common qemuarm64/virt-machine second
# port) — confirm against meta/conf/machine/qemuarm64.conf + a live boot's `dmesg | grep tty`/
# `ls /dev/tty*` before trusting this for a real CI run. See the NETWORK/SERIAL WIRING comment above
# for the single-UART fallback if qemuarm64 truly exposes only one.
SERIAL_GUEST_DEV="${YOCTO_E2E_SERIAL_GUEST_DEV:-/dev/ttyS1}"

log() { printf '\033[1;35m[yocto-e2e]\033[0m %s\n' "$*" >&2; }

# ---------------------------------------------------------------------------------------------
# up / down: the warm long-lived container.
# ---------------------------------------------------------------------------------------------

is_running() { docker ps --format '{{.Names}}' | grep -qx "$CONTAINER_NAME"; }
exists() { docker ps -a --format '{{.Names}}' | grep -qx "$CONTAINER_NAME"; }

cmd_up() {
  mkdir -p "$CACHE_DIR"
  if is_running; then
    log "container '$CONTAINER_NAME' already running — reusing it."
    return 0
  fi
  if exists; then
    log "container '$CONTAINER_NAME' exists but is stopped — starting it."
    docker start "$CONTAINER_NAME" >/dev/null
    return 0
  fi
  log "starting warm container '$CONTAINER_NAME' (cache: $CACHE_DIR)"
  # No --cap-add NET_ADMIN / --device /dev/net/tun here (unlike qemu-e2e.sh): this harness uses
  # slirp user-net (runqemu's `slirp` mode) + TCP-loopback chardevs, not a TAP device, so no
  # elevated network capabilities are needed on the container itself. runqemu's `nographic` mode
  # also needs no /dev/kvm passthrough requirement beyond whatever sentient-yocto-builder already
  # provides for TCG (software) emulation — KVM is a nice-to-have speed-up, not a hard requirement.
  docker run -d --name "$CONTAINER_NAME" \
    --add-host rust-mirror:10.0.0.192 \
    -v "$CACHE_DIR:/build" \
    -v "$REPO_ROOT:/repo" \
    "$IMAGE" sleep infinity >/dev/null
  log "container started."
}

cmd_down() {
  if exists; then
    log "stopping + removing container '$CONTAINER_NAME'"
    docker rm -f "$CONTAINER_NAME" >/dev/null
  else
    log "container '$CONTAINER_NAME' does not exist — nothing to do."
  fi
}

require_up() {
  if ! is_running; then
    log "container not running — starting it now (use 'yocto-e2e.sh up' to do this explicitly)."
    cmd_up
  fi
}

dexec() {
  # Plain `bash -c` (NOT `bash -lc`) — same reasoning as qemu-e2e.sh: a login shell re-sources
  # /etc/profile and can clobber the image's baked-in PATH (kas + BitBake live under the `builder`
  # user's env in .ci/yocto.Dockerfile, not necessarily on a login-shell default PATH).
  docker exec -u builder "$CONTAINER_NAME" bash -c "$1"
}

# ---------------------------------------------------------------------------------------------
# setup: one-time deps into the warm container (paho-mqtt venv for the external MQTT assertion
# client). kas + BitBake host deps are ALREADY baked into sentient-yocto-builder
# (.ci/yocto.Dockerfile) — this step only adds what THAT image doesn't already carry.
# ---------------------------------------------------------------------------------------------

cmd_setup() {
  require_up
  local sentinel=/build/.yocto-e2e-setup-done
  if dexec "test -f $sentinel"; then
    log "warm-container deps already installed (sentinel found) — skipping."
    return 0
  fi
  log "installing deps into the warm container (one-time)"
  # No `|| true` masking here either (same discipline as qemu-e2e.sh) — a partial/failed install
  # must abort loudly rather than silently mark the sentinel done.
  dexec '
    set -euo pipefail
    [ -d /build/venv ] || python3 -m venv /build/venv
    /build/venv/bin/pip install -q --upgrade pip
    /build/venv/bin/pip install -q paho-mqtt
  '
  if ! dexec '
    set -euo pipefail
    command -v kas >/dev/null || { echo "MISSING: kas (should be baked into sentient-yocto-builder)" >&2; exit 1; }
    /build/venv/bin/python3 -c "import paho.mqtt.client" 2>/dev/null \
        || { echo "MISSING: python paho-mqtt in /build/venv" >&2; exit 1; }
  '; then
    log "ERROR: dependency verification FAILED after install — NOT writing the setup sentinel."
    log "       re-run 'yocto-e2e.sh setup' (or 'build'/'run') to retry the install."
    return 1
  fi
  dexec "touch $sentinel"
  log "warm-container deps installed and verified."
}

# ---------------------------------------------------------------------------------------------
# sync: copy /repo into /build/ws/repo, EXCLUDING any BitBake output so a stale local build/ never
# leaks into the synced copy. The west/Zephyr excludes (zephyr, modules, .west) are vestigial — the
# MCU firmware tree they guarded against was deleted with the Embassy pivot
# (docs/decisions/0010-mcu-runtime-embassy.md) — but are kept as cheap, harmless no-ops. Same
# tar-based approach as the retired MCU harness's cmd_sync (cheap, skips .git entirely during the
# copy instead of copying+deleting it).
# ---------------------------------------------------------------------------------------------

cmd_sync() {
  require_up
  log "syncing /repo -> /build/ws/repo (excludes: target,.git,build,zephyr,modules,tools,bootloader,.west,yocto-cache)"
  dexec '
    set -euo pipefail
    mkdir -p /build/ws/repo
    tar -C /repo \
      --exclude=target --exclude=.git --exclude=build --exclude=zephyr --exclude=modules \
      --exclude=tools --exclude=bootloader --exclude=.west --exclude=yocto-cache \
      -cf - . | tar -C /build/ws/repo -xf -
  '
}

# ---------------------------------------------------------------------------------------------
# kas config resolution: qemuarm64 has NO dedicated kas config-layer file today (only
# distro/kas/sentient-rpi4.yml exists, for the raspberrypi4-64 SHIPPING machine — owned by another
# workstream). Rather than add new files outside this script's ownership, this harness uses kas's
# own documented `KAS_MACHINE` env-var override (see distro/kas/sentient.yml's header: "Override
# per-build with `KAS_MACHINE=raspberrypi4-64` (env)") against the bare `sentient.yml` manifest —
# no new kas/*.yml file needed for the emulation-proxy machines.
# ---------------------------------------------------------------------------------------------

kas_config_for() {
  case "$1" in
    qemuarm64|qemuarm) echo "distro/kas/sentient.yml" ;;
    raspberrypi4-64) echo "distro/kas/sentient.yml:distro/kas/sentient-rpi4.yml" ;;
    *) echo "distro/kas/sentient.yml" ;;
  esac
}

# ---------------------------------------------------------------------------------------------
# build: sync + `kas build ... --target sentient-image` for MACHINE, sstate/downloads cached in
# /build/yocto-cache (bind-mounted so it survives across invocations — this is THE thing that makes
# a Yocto e2e loop tractable at all; see .ci/yocto.Dockerfile's own comment on why sstate reuse is
# mandatory, not optional).
# ---------------------------------------------------------------------------------------------

cmd_build() {
  local machine="${1:-$MACHINE}"
  require_up
  cmd_setup
  cmd_sync
  local kas_cfg
  kas_cfg="$(kas_config_for "$machine")"
  log "kas build $kas_cfg (KAS_MACHINE=$machine) --target sentient-image"
  # DL_DIR/SSTATE_DIR point at the SAME /build-mounted cache dir every invocation, keyed only by
  # content (kas/BitBake's own sstate hashing) — no need to vary the host path per machine, BitBake
  # partitions its own sstate by machine/arch internally.
  dexec "
    set -euo pipefail
    export KAS_MACHINE='$machine'
    export DL_DIR=/build/yocto-cache/downloads
    export SSTATE_DIR=/build/yocto-cache/sstate-cache
    mkdir -p \"\$DL_DIR\" \"\$SSTATE_DIR\"
    cd /build/ws/repo
    kas build $kas_cfg --target sentient-image
  "
  log "build complete (machine=$machine): /build/ws/repo/build/tmp/deploy/images/$machine/"
}

# ---------------------------------------------------------------------------------------------
# sim-bin: build seplos-tcp-sim once, using the REAL /repo mount + cargo's own target/ dir (a plain
# host cargo build, no BitBake/kas involvement) — identical rationale to qemu-e2e.sh's cmd_sim_bin.
# ---------------------------------------------------------------------------------------------

cmd_sim_bin() {
  require_up
  log "building seplos-tcp-sim (test-kit, tcp-sim feature)"
  dexec '
    set -euo pipefail
    cd /repo
    cargo build -p test-kit --features tcp-sim --bin seplos-tcp-sim
  '
  log "sim binary: $REPO_ROOT/target/debug/seplos-tcp-sim"
}

# ---------------------------------------------------------------------------------------------
# e2e config: render the e2e-only sentient.toml (broker widened to 0.0.0.0, serial pointed at the
# guest UART) and inject it into the just-built image's rootfs at /etc/sentient/sentient.toml
# BEFORE boot — NOT at runtime — since the image ships read-only-rootfs (Phase 1H hardening; see
# sentient-image.bb) with no writable overlay guaranteed at the point systemd starts
# sentient-agent.service. runqemu supports an rootfs-overlay/`--overlay` mechanism in some Yocto
# releases; the more portable, version-independent approach used here is to unpack the just-built
# `.wic`'s root partition, drop the file in, and repack — done via Yocto's own `wic cp`
# sub-command (part of the `wic` tool BitBake/poky ships), which is designed for exactly this
# ("copy a file into/out of a wic image's partition without a full loopback mount").
#
# TODO(verify-at-build): confirm `wic cp <file> <image.wic>:/etc/sentient/sentient.toml` is
# available/works for the exact wic image this recipe produces (ext4 rootfs partition; `wic cp`
# needs guestfish/libguestfs OR a loopback mount depending on the wic plugin — meta-sentient's
# read-only-rootfs IMAGE_FEATURE should not block a *build-time* rootfs write via wic tooling, only
# the *running* image's own mount). If `wic cp` proves unavailable/unreliable in
# sentient-yocto-builder, the fallback is a BitBake-side mechanism instead (an e2e-only bbappend on
# sentient-agent that installs this file under ${sysconfdir}/sentient/sentient.toml, gated behind a
# DISTRO_FEATURE or a separate e2e image recipe) — that fallback is NOT implemented here since it
# would mean editing distro/meta-sentient/** (out of this script's ownership); flagging the decision
# point for whoever verifies this at build time.
# ---------------------------------------------------------------------------------------------

render_e2e_config() {
  cat <<EOF
[device]
id = "METEOR-001"

[serial]
kind = "rs485"
port = "$SERIAL_GUEST_DEV"
baud = 19200
parity = "none"
stop_bits = 1
read_timeout_ms = 1000

[poll]
dip_addresses = [0]
interval_ms = 1000
fault_every = 10

[broker]
# WIDENED FOR THIS E2E ONLY (isolated QEMU slirp segment) — a real device keeps 127.0.0.1. See
# this script's header comment + distro/qemu/README.md for why this is safe here and not a template
# to copy into a real deployment (no CONNECT authorizer backend exists yet in lib-security).
host = "0.0.0.0"
port = 1883

[publish]
prefix = "tms"
EOF
}

inject_e2e_config() {
  local machine="$1"
  local deploy_dir="/build/ws/repo/build/tmp/deploy/images/$machine"
  log "injecting e2e sentient.toml into the built image ($deploy_dir)"
  dexec "
    set -euo pipefail
    DEPLOY='$deploy_dir'
    cat > /build/e2e-sentient.toml <<'CFGEOF'
$(render_e2e_config)
CFGEOF
    shopt -s nullglob
    cd \"\$DEPLOY\"
    WIC=\$(ls sentient-image-*.wic 2>/dev/null | head -n1)
    if [ -z \"\$WIC\" ]; then
      # A compressed-only output (.wic.bz2) still needs a decompressed .wic for wic cp / runqemu.
      WICBZ=\$(ls sentient-image-*.wic.bz2 2>/dev/null | head -n1)
      if [ -n \"\$WICBZ\" ]; then
        bunzip2 -k \"\$WICBZ\"
        WIC=\"\${WICBZ%.bz2}\"
      fi
    fi
    test -n \"\$WIC\" || { echo 'no .wic image found under '\"\$DEPLOY\"; exit 1; }
    wic cp /build/e2e-sentient.toml \"\${WIC}:/etc/sentient/sentient.toml\"
    echo \"[yocto-e2e] injected e2e config into \$WIC\"
  "
}

# ---------------------------------------------------------------------------------------------
# run: boot under runqemu headless with slirp hostfwd (MQTT) + a chardev-attached second UART
# (Seplos serial), drive seplos-tcp-sim against the guest, and assert TMS payloads via an external
# paho MQTT client. Mirrors qemu-e2e.sh's cmd_run structure (early-connect + bounded retry for the
# sim, a single external-client "SUMMARY"/PASS-FAIL gate) but is NOT the same command sequence —
# runqemu replaces the bare qemu-system-* invocation, and TCP-loopback replaces the TAP.
# ---------------------------------------------------------------------------------------------

cmd_run() {
  local n="${1:-1}"
  local machine="${2:-$MACHINE}"
  require_up
  cmd_build "$machine"
  inject_e2e_config "$machine"
  cmd_sim_bin
  for i in $(seq 1 "$n"); do
    log "=== e2e run $i/$n (machine=$machine) ==="
    dexec "
      set -uo pipefail
      export KAS_MACHINE='$machine'

      # runqemu is part of the poky checkout kas clones as a sibling of build/ — source its
      # environment-setup script so 'runqemu' resolves on PATH the standard Yocto way.
      # TODO(verify-at-build): confirm the poky checkout actually lands at /build/ws/repo/poky —
      # this is kas's documented default repo layout (fetched layers as siblings of the invoking
      # dir), but has not been observed against a real kas run in this environment.
      cd /build/ws/repo
      source poky/oe-init-build-env build >/dev/null

      rm -f /build/console.log

      # slirp user-net + hostfwd for MQTT (1883 in the guest -> $MQTT_HOST_PORT on the host/container),
      # nographic + serial console to a logfile, plus a SECOND -serial chardev socket (server=on,
      # wait=off — QEMU is the server, seplos-tcp-sim dials in as the client, exactly like
      # qemu-e2e.sh's '-serial tcp:...,server,nowait' pattern) for the Seplos link.
      #
      # TODO(verify-at-build): runqemu's own 'slirp' + 'serial' convenience keywords already imply
      # a default hostfwd/console wiring for SOME ports; passing an explicit -netdev/-serial via
      # qemuparams= alongside those keywords needs confirming it doesn't fight/duplicate what the
      # keyword already adds (see distro/qemu/README.md's citation notes). If it conflicts, drop
      # the 'slirp'/'serial' bare keywords and pass -netdev/-serial entirely via qemuparams instead.
      runqemu '$machine' nographic slirp \
        qemuparams=\"-netdev user,id=net0,hostfwd=tcp::${MQTT_HOST_PORT}-:1883 -device virtio-net-device,netdev=net0 -chardev socket,id=seplossim,host=127.0.0.1,port=${SERIAL_HOST_PORT},server=on,wait=off -serial chardev:seplossim\" \
        > /build/console.log 2>&1 &
      QEMU_PID=\$!
      echo \"[yocto-e2e] runqemu PID=\$QEMU_PID\"

      # Boot-to-ready wait: poll the console log for the branded agent-ready banner (see
      # distro/meta-sentient's sentient-agent.service ExecStartPost / notify_ready() wiring —
      # CLAUDE.md Phase 1 'Agent-ready banner'). Bounded to avoid hanging forever on a boot fault.
      # TODO(verify-at-build): confirm the EXACT banner text once that ExecStartPost lands (owned
      # by another workstream); 'sentient agent ready' is this script's working assumption per the
      # plan's own wording. Fall back to a raw systemd-unit-active poll if the banner text differs.
      echo '[yocto-e2e] waiting for boot / agent-ready banner (up to 180s)...'
      BOOT_OK=0
      for _try in \$(seq 1 90); do
        if grep -qiE 'sentient agent ready|login:' /build/console.log 2>/dev/null; then
          BOOT_OK=1
          break
        fi
        sleep 2
      done
      if [ \"\$BOOT_OK\" -ne 1 ]; then
        echo '[yocto-e2e] WARNING: no agent-ready/login banner seen within 180s — proceeding anyway (console dump below will show why).'
      fi

      # Connect the sim EARLY and retry-bound, same rationale as qemu-e2e.sh: connecting before the
      # firmware/agent's first poll cycle keeps its retry/backoff shallow.
      for _try in \$(seq 1 20); do
        /repo/target/debug/seplos-tcp-sim --connect 127.0.0.1:${SERIAL_HOST_PORT} > /build/sim.log 2>&1 &
        SIM_PID=\$!
        sleep 0.5
        if kill -0 \$SIM_PID 2>/dev/null; then
          break
        fi
      done
      echo \"[yocto-e2e] seplos-tcp-sim PID=\$SIM_PID\"

      sleep 2
      echo '=== console so far ==='
      tail -c 4000 /build/console.log || true

      echo '=== external MQTT client proof (tms/# + monitor/#, via slirp hostfwd) ==='
      set +e
      /build/venv/bin/python3 /repo/distro/qemu/lib/tms_mqtt_check.py \
        127.0.0.1 ${MQTT_HOST_PORT} yocto-e2e-run-$i 120
      RC=\$?
      set -e

      echo '=== final console dump ==='
      tail -c 8000 /build/console.log || true

      kill \$SIM_PID \$QEMU_PID 2>/dev/null || true
      sleep 2
      kill -9 \$SIM_PID \$QEMU_PID 2>/dev/null || true
      # runqemu spawns helper processes (qemu-system-* itself, a runqemu-flock etc.) that a plain
      # kill of the wrapper PID may not reap. Best-effort cleanup so a stuck instance doesn't block
      # the NEXT invocation from binding the same hostfwd/serial ports.
      pkill -f 'qemu-system' 2>/dev/null || true
      exit \$RC
    "
  done
}

# ---------------------------------------------------------------------------------------------

usage() {
  cat >&2 <<EOF
usage: $0 <up|down|setup|sync|build [MACHINE]|sim-bin|run [N] [MACHINE]>

MACHINE defaults to \$YOCTO_E2E_MACHINE or 'qemuarm64' (ARM only — x86 is not supported).
EOF
  exit 1
}

case "${1:-}" in
  up) cmd_up ;;
  down) cmd_down ;;
  setup) cmd_setup ;;
  sync) cmd_sync ;;
  build) shift; cmd_build "${1:-$MACHINE}" ;;
  sim-bin) cmd_sim_bin ;;
  run) shift; cmd_run "${1:-1}" "${2:-$MACHINE}" ;;
  *) usage ;;
esac

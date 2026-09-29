# --- Base/app split (systemd-sysext) ------------------------------------------------------
# This recipe now produces TWO packages instead of one, splitting "the OS-side plumbing"
# from "the app binary" so the binary can be delivered independently as a systemd-sysext
# extension image (see recipes-sentient/sentient-app-sysext/sentient-app-sysext.bb):
#
#   - sentient-agent-bin (${PN}-bin): ONLY /usr/bin/bin-rpi. This package is intentionally
#     NOT installed into sentient-image's base rootfs; it exists so sentient-app-sysext can
#     pull it into the extension image instead. Kept in THIS recipe (rather than a new one)
#     because it still shares SRC_URI/do_install/do_compile with sentient-agent — only
#     packaging (FILES/PACKAGES) diverges.
#   - sentient-agent (${PN}): the systemd unit, the "sentient" system user/group, and the
#     default /etc/sentient/sentient.toml config. This is what sentient-image.bb installs
#     into the BASE image. It deliberately does NOT contain /usr/bin/bin-rpi and does NOT
#     RDEPEND on sentient-agent-bin: the binary is not present in the base rootfs at all —
#     it arrives at runtime only via the sysext overlay merged by systemd-sysext.service
#     (see sentient-image.bb's systemd-sysext enablement). ExecStart=/usr/bin/bin-rpi in
#     sentient-agent.service therefore resolves at runtime against the MERGED
#     (base rootfs + sysext) view of /usr, not against anything this package ships — the
#     unit will fail to start until the sysext is merged, which is expected/by-design.
#
# This lets the app binary be updated (new sysext image) without rebuilding/re-signing the
# base OS image, and lets the base image be rebuilt (security patches, kernel bumps) without
# re-touching the app artifact. See distro/README.md and CLAUDE.md's sysext split note.

SUMMARY = "Sentient METEOR TMS telemetry adapter (bin-rpi binary + hardened systemd unit)"
DESCRIPTION = "Packages the prebuilt, signed aarch64-unknown-linux-musl bin-rpi binary \
(Seplos BMS -> TMS -> embedded MQTT broker) and a hardened systemd unit, split into a \
base-image package (unit + user + config) and an app-sysext package (the binary only). \
This recipe never invokes cargo/rustc: bin-rpi is built and signed by the existing release \
pipeline (release.yml) and consumed here as a prebuilt artifact — see distro/README.md."
HOMEPAGE = "https://github.com/highvelocitydecisions/sentient-rs"
SECTION = "console/network"

# The Rust source is proprietary and its license is not being re-derived here; this recipe
# only repackages an already-built, already-signed artifact.
LICENSE = "CLOSED"

# Prebuilt binary recipes still need do_compile to be a no-op (nothing to build).
inherit systemd useradd

# musl-static aarch64 binary; only makes sense on an aarch64 Linux host.
COMPATIBLE_HOST = "aarch64.*-linux"

# ---------------------------------------------------------------------------------------
# SRC_URI: the prebuilt artifact.
#
# TODO(verify-at-build): this is a GREENFIELD placeholder. In the real pipeline, release.yml
# (Sub-task 2, "musl-static release artifact") produces a signed
# aarch64-unknown-linux-musl bin-rpi and CI wires its real fetch URI (signed release asset
# URL, X.509 CMS-verified via its .p7s against CA_CRT, e.g. via SRC_URI[sha256sum] + a fetch/verify
# wrapper, or a
# BB_FETCH-compatible mechanism resolving to a pinned release tag) into this recipe. Until
# that lands, SRC_URI points at a local placeholder checked in under files/ purely so the
# recipe skeleton is syntactically complete and self-documenting.
SRC_URI = "file://bin-rpi \
           file://sentient-agent.service \
           file://sentient.toml"

# TODO(verify-at-build): replace with the real sha256sum of the signed release artifact
# once release.yml wires the fetch (BitBake requires a checksum for non-local/network
# SRC_URI entries; a file:// entry checked into this layer doesn't strictly need one, but
# the eventual network SRC_URI will).
SRC_URI[sha256sum] = "0000000000000000000000000000000000000000000000000000000000000000"

S = "${UNPACKDIR}"

# --- Package split: order matters ------------------------------------------------------
# sentient-agent-bin MUST be listed before ${PN} so its explicit FILES claim
# (${bindir}/bin-rpi) is assigned before ${PN}'s own FILES globs are evaluated — BitBake
# packages files into the FIRST package (in PACKAGES order) whose FILES pattern matches,
# so an explicit narrow claim ahead of a broader one is how the binary is kept out of ${PN}.
PACKAGES =+ "${PN}-bin"

# Already built, already stripped (release profile + `strip` in the existing pipeline),
# already musl-static: nothing for BitBake's own strip/sysroot-strip/QA passes to do, and
# ldflags QA (which expects a recompiled, dynamically-linked ELF) doesn't apply to a
# prebuilt static binary. The `already-stripped` QA scan is evaluated at the RECIPE (${PN})
# level, not per-subpackage — so this MUST be set on ${PN}, not just ${PN}-bin (scoping it
# only to -bin let do_package fail with "File '/usr/bin/bin-rpi' from sentient-agent was
# already stripped [already-stripped]" once a real stripped bin-rpi was staged). Set on both
# to also cover any per-package QA pass.
INSANE_SKIP:${PN} += "ldflags already-stripped"
INSANE_SKIP:${PN}-bin += "ldflags already-stripped"
INHIBIT_PACKAGE_STRIP = "1"
INHIBIT_SYSROOT_STRIP = "1"

# sentient-agent-bin: the binary ONLY. Must not be empty (it's pointless without the
# binary), so require it explicitly rather than relying on the ALLOW_EMPTY default.
ALLOW_EMPTY:${PN}-bin = "0"
FILES:${PN}-bin = "${bindir}/bin-rpi"

# sentient-agent (${PN}): unit + user/group + config ONLY. No binary here, and no RDEPENDS
# on sentient-agent-bin — see the header comment above for why (the binary is a runtime
# sysext overlay concern, not a base-package dependency).
FILES:${PN} = "${systemd_system_unitdir}/sentient-agent.service \
               ${sysconfdir}/sentient"

# Static "sentient" system user: owns the serial tty (via the dialout group) and any
# runtime state (StateDirectory=sentient in the unit). A static UID/GID — NOT
# systemd DynamicUser — because a serial-port + state-directory owner must have a stable
# identity across reboots/upgrades (recycled dynamic UIDs would strand ownership).
USERADD_PACKAGES = "${PN}"
GROUPADD_PARAM:${PN} = "--system sentient"
USERADD_PARAM:${PN} = "--system --no-create-home --shell /usr/sbin/nologin \
                       --gid sentient -G dialout sentient"

SYSTEMD_PACKAGES = "${PN}"
SYSTEMD_SERVICE:${PN} = "sentient-agent.service"
# Boot the agent by default on first boot of the image (no manual `systemctl enable` step).
# NOTE: the unit will fail to start (ExecStart=/usr/bin/bin-rpi missing) until the
# sentient-app-sysext extension is merged by systemd-sysext.service — Restart=on-failure
# in the unit itself covers the boot-ordering window (systemd-sysext merges before
# basic.target; see sentient-image.bb).
SYSTEMD_AUTO_ENABLE:${PN} = "enable"

do_install() {
    install -d ${D}${bindir}
    # TODO(verify-at-build): once SRC_URI fetches the real signed artifact, confirm the
    # fetched file's on-disk name here still matches (release.yml's current artifact name
    # is bin-rpi; keep this in lockstep with that job's `actions/upload-artifact` name).
    install -m 0755 ${UNPACKDIR}/bin-rpi ${D}${bindir}/bin-rpi

    install -d ${D}${systemd_system_unitdir}
    install -m 0644 ${UNPACKDIR}/sentient-agent.service ${D}${systemd_system_unitdir}/sentient-agent.service

    # Runtime config at /etc/sentient/sentient.toml (matches sentient-agent.service's
    # SENTIENT_CONFIG + docs/deployment.md §4). Ship a working default so the agent STARTS on
    # first boot — without it bin-rpi deserializes an empty config and exits 1 ("missing field
    # `device`"), restart-looping the unit. Operators override per-device (or via SENTIENT_* env).
    # Kept in sync with the canonical repo config/sentient.toml by _yocto.yml's stage step.
    install -d ${D}${sysconfdir}/sentient
    install -m 0644 ${UNPACKDIR}/sentient.toml ${D}${sysconfdir}/sentient/sentient.toml
}

CONFFILES:${PN} += "${sysconfdir}/sentient/sentient.toml"

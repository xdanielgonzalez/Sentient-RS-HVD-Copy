# sentient-app-sysext.bb — the systemd-sysext APP EXTENSION image.
#
# --- What a sysext is ---------------------------------------------------------------------
# A systemd-sysext (https://www.freedesktop.org/software/systemd/man/latest/systemd-sysext.html)
# is a small, separately-built/signed filesystem image that systemd-sysext.service
# overlays (via overlayfs) on top of the base OS's /usr and /opt at boot, BEFORE
# basic.target — the merged view is what the rest of the system (including our own
# sentient-agent.service) actually sees. It lets an "app" payload (here: just the bin-rpi
# binary) be built, versioned, signed, and shipped on its OWN cadence, independent of the
# immutable base OS image (sentient-image.bb) that carries the kernel, systemd, mosquitto,
# and the sentient-agent unit/user/config.
#
# --- Phase-1 scope (this recipe) ------------------------------------------------------
# This recipe produces the UNSIGNED extension rootfs/image only:
#   - CMS signing of the resulting image is handled OUT OF BAND by another stage of the
#     release pipeline (X.509 CMS, mirroring the base .wic's signing story) — NOT this
#     recipe's concern.
#   - dm-verity (block-level integrity for the extension image) is DEFERRED, same as the
#     base image's dm-verity follow-up (CLAUDE.md Phase 3) — oe-core wrynose does not ship
#     dm-verity image tooling, so it is out of scope here too.
# In other words: this recipe's job is "produce a correctly-shaped, correctly-labeled
# sysext squashfs"; making it trusted/immutable is downstream pipeline work.
#
# --- The SYSEXT_LEVEL match contract ----------------------------------------------------
# systemd-sysext matches an extension-release against the base OS's /etc/os-release by ID
# (always) and then by EITHER SYSEXT_LEVEL (if set on both sides) OR VERSION_ID (fallback).
# When SYSEXT_LEVEL is present on both sides it takes precedence over VERSION_ID — this is
# deliberate here: it lets this app extension be rebuilt/re-versioned independently of the
# base OS's DISTRO_VERSION, as long as the SYSEXT_LEVEL "ABI" the base image promises (see
# the os-release.bbappend in recipes-core/os-release/) hasn't changed. Contract (frozen):
#   base sentient-image  os-release:            ID=sentient, VERSION_ID=1.0.0, SYSEXT_LEVEL=1
#   this extension-release:  ID=sentient, SYSEXT_LEVEL=1, ARCHITECTURE=arm64, EXTENSION_RELOAD_MANAGER=1
# sysext-image.bbclass (meta-openembedded/meta-oe, already on BBLAYERS) writes ID/VERSION_ID/
# EXTENSION_RELOAD_MANAGER for us; it does NOT write SYSEXT_LEVEL or ARCHITECTURE, so this
# recipe's ROOTFS_POSTPROCESS_COMMAND below appends those two explicitly.

SUMMARY = "Sentient METEOR app sysext: bin-rpi binary only, merged onto the base OS at boot"
DESCRIPTION = "systemd-sysext extension image carrying ONLY the sentient-agent-bin package \
(/usr/bin/bin-rpi). Overlaid onto the base sentient-image rootfs by systemd-sysext.service \
at boot, decoupling app-binary updates from base-OS image rebuilds. See the header comment \
above for the SYSEXT_LEVEL match contract and distro/README.md for the overall image split."
HOMEPAGE = "https://github.com/highvelocitydecisions/sentient-rs"

# Same posture as sentient-agent_0.1.0.bb: this repackages an already-built, already-signed
# (out-of-band) proprietary artifact; no source license to declare here.
LICENSE = "CLOSED"

# sysext-image.bbclass: inherits `image`, writes
# ${IMAGE_ROOTFS}/usr/lib/extension-release.d/extension-release.${EXTENSION_NAME} with
# ID=${DISTRO} / VERSION_ID=${DISTRO_VERSION} (sanitized) / EXTENSION_RELOAD_MANAGER=1,
# sets the user.extension-release.strict=false xattr, and adds
# PACKAGE_EXCLUDE += "os-release" (the extension does not ship its own top-level
# /etc/os-release — only the extension-release.d fragment). It does NOT set IMAGE_FSTYPES
# and does NOT write SYSEXT_LEVEL/ARCHITECTURE — both handled explicitly below.
inherit sysext-image

# TODO(verify-at-build): confirm this is the actual class-provided variable name for the
# extension's ID field (vs. it simply being derived unconditionally from ${DISTRO} with no
# override knob at all) — `bitbake -e sentient-app-sysext | grep -i extension` against the
# real sysext-image.bbclass source once it's checked out. ${DISTRO} already resolves to
# "sentient" (see conf/distro/sentient.conf) so this is set explicitly here only for
# recipe-level clarity/self-documentation, not because the default is believed to be wrong.
EXTENSION_ID_FIELD = "sentient"

# EXTENSION_NAME names the in-image metadata file (extension-release.${EXTENSION_NAME}) AND is what
# systemd-sysext matches against the deployed .raw's filename. The class default
# ("${IMAGE_LINK_NAME}.${IMAGE_FSTYPES}" = sentient-app-sysext-<machine>.sysext.squashfs) only
# matches a same-named .raw; a renamed .raw relies on the class's user.extension-release.strict=false
# xattr, which did NOT survive into the squashfs — UAT boot: systemd-sysext[]: "Failed to read
# metadata for image sentient-app: No such file or directory". We deploy the extension as
# /var/lib/extensions/sentient-app.raw (uat.yml + on device), so pin EXTENSION_NAME to "sentient-app"
# — the metadata file becomes extension-release.sentient-app, an EXACT match for the deployed
# filename with no xattr dependency.
EXTENSION_NAME = "sentient-app"

# Only the app binary package goes into the extension — nothing else. sentient-agent-bin
# is produced by sentient-agent_0.1.0.bb's package split (see that recipe's header comment).
IMAGE_INSTALL = "sentient-agent-bin"

# A single-file squashfs is the standard systemd-sysext image format (read-only, compact,
# no filesystem journal/metadata overhead for what is essentially one binary).
#
# IMAGE_FSTYPES MUST stay a SINGLE fstype here: sysext-image.bbclass derives
#   EXTENSION_NAME = "${IMAGE_LINK_NAME}.${IMAGE_FSTYPES}"
# and then runs `install … extension-release.${EXTENSION_NAME}`. The kas manifest appends "ext4"
# to IMAGE_FSTYPES GLOBALLY (sentient-image-fstype fragment, for the base image's runqemu-bootable
# rootfs), which would make IMAGE_FSTYPES "squashfs ext4" here — a SPACE in EXTENSION_NAME turns
# that install into `install SRC extension-release.<...>.squashfs ext4`, failing do_rootfs with
# "install: target 'ext4': No such file or directory". Drop the global ext4 append for the sysext.
IMAGE_FSTYPES = "squashfs"
IMAGE_FSTYPES:remove = "ext4"

# TODO(verify-at-build): confirm the exact emitted image filename
# (deploy/images/${MACHINE}/sentient-app-sysext-${MACHINE}.squashfs, or with a
# DATETIME/version suffix per sysext-image.bbclass's own naming) — needed by the pipeline
# stage that CMS-signs and stages this artifact for delivery/activation
# (systemd-sysext refresh / drop into /var/lib/extensions or /run/extensions).

# --- SYSEXT_LEVEL + ARCHITECTURE: appended defensively -----------------------------------
# sysext-image.bbclass writes extension-release.${EXTENSION_NAME} with ID/VERSION_ID/
# EXTENSION_RELOAD_MANAGER but NOT SYSEXT_LEVEL or ARCHITECTURE. Append both post-rootfs,
# globbing the actual filename (EXTENSION_NAME may not exactly equal PN) and guarding
# against double-appending (e.g. a re-run of this task on an already-processed rootfs)
# by only adding a key if it is not already present.
#
# NOTE: the function name must NOT contain the substring `_append` (or `_prepend`/`_remove`) —
# modern BitBake parses those as the deprecated override operators and hard-fails at parse
# ("Variable ... contains an operation using the old override syntax"). Hence the neutral name.
ROOTFS_POSTPROCESS_COMMAND += "sentient_sysext_release_fields; "

sentient_sysext_release_fields () {
    for f in ${IMAGE_ROOTFS}/usr/lib/extension-release.d/extension-release.*; do
        [ -e "$f" ] || continue
        grep -q '^SYSEXT_LEVEL=' "$f" || echo 'SYSEXT_LEVEL=1' >> "$f"
        grep -q '^ARCHITECTURE=' "$f" || echo 'ARCHITECTURE=arm64' >> "$f"
    done
}

# Same shipping/emulation machine set as sentient-image.bb — the extension must be
# buildable (and mergeable) everywhere the base image is.
COMPATIBLE_MACHINE = "raspberrypi4-64|raspberrypi5|qemuarm64|qemuarm"

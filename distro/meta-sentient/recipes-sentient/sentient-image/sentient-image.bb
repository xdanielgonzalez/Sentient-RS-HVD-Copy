SUMMARY = "Sentient METEOR minimal Raspberry Pi image (bin-rpi + mosquitto bridge, hardened)"
DESCRIPTION = "A minimal Yocto Linux image for the Sentient METEOR Raspberry Pi comms \
module: systemd init, no package manager, no dev/debug tooling, read-only rootfs, running \
the prebuilt bin-rpi agent and a TLS-hardened mosquitto cloud-relay bridge. See \
distro/README.md and docs/deployment.md for the runtime architecture this packages."
HOMEPAGE = "https://github.com/highvelocitydecisions/sentient-rs"
LICENSE = "MIT"
# TODO(verify-at-build): core-image-minimal itself is typically MIT-or-similar via poky's
# common-licenses; confirm this image recipe's own LICENSE field convention (some
# Yocto image recipes omit LICENSE entirely and rely on per-package license manifests —
# check how other core-image-*.bb recipes in the pinned poky/meta-raspberrypi declare it).

# Base ourselves on core-image-minimal's package set/policy rather than reinventing it;
# `require` (not `inherit core-image` alone) pulls in its IMAGE_INSTALL baseline too.
require recipes-core/images/core-image-minimal.bb

# --- Our packages ------------------------------------------------------------------------
# sentient-agent: unit + "sentient" user/group + default config ONLY (see
# recipes-sentient/sentient-agent/sentient-agent_0.1.0.bb's package-split header comment).
# As of the base/app sysext split, the bin-rpi binary itself is DELIBERATELY NOT part of
# this base image — it is packaged separately as sentient-agent-bin and delivered at
# runtime by the sentient-app-sysext extension (recipes-sentient/sentient-app-sysext/),
# merged onto /usr by systemd-sysext.service before basic.target (enabled below). This
# lets the app binary be updated/re-signed without rebuilding the base OS image.
# mosquitto: the cloud-relay bridge (see recipes-connectivity/mosquitto/) — package name
# from meta-openembedded/meta-networking.
IMAGE_INSTALL:append = " sentient-agent mosquitto"

# --- systemd-sysext enablement (base/app image split) -----------------------------------
# The systemd sysext PACKAGECONFIG (which builds systemd-sysext.service + the CLI) is enabled in a
# systemd BBAPPEND — recipes-core/systemd/systemd_%.bbappend — NOT here. A
# `PACKAGECONFIG:append:pn-systemd` set in this image recipe is a no-op (recipe-scoped `:pn-*`
# overrides don't cross into systemd's own parse), so the unit was never built. The wants-symlink
# enablement below assumes the bbappend has made the unit present.

# Enable systemd-sysext.service itself so extensions under /var/lib/extensions (or
# /run/extensions) are auto-discovered and overlay-merged onto /usr at EARLY boot (it runs
# before basic.target, so sentient-agent.service — which is `After=network-online.target`,
# itself well after basic.target — always sees the merged view).
# TODO(verify-at-build): confirm the exact unit/subpackage name carrying
# systemd-sysext.service in the pinned wrynose systemd recipe once the `sysext`
# PACKAGECONFIG above is enabled (`bitbake -e systemd | grep -i sysext`), and confirm
# whether it ships enabled-by-default via its own upstream preset (many systemd .service
# units under systemd's own control enable via `systemctl preset-all` / the shipped
# /usr/lib/systemd/system-preset/ files rather than needing SYSTEMD_AUTO_ENABLE here) —
# if it does NOT auto-enable, add a SYSTEMD_AUTO_ENABLE:pn-systemd = "enable" (or the
# equivalent for whichever subpackage carries the unit) below this comment.
# systemd-sysext.service ships `[Install] WantedBy=sysinit.target` (verified from systemd v259.5
# source), and upstream's 90-systemd.preset says "enable" it — but the first UAT boot proved that
# NEITHER the stock OE `preset-all` pass NOR SYSTEMD_AUTO_ENABLE actually enabled it (journal: no
# merge, "System is tainted: unmerged-bin"). The `systemd` package (${PN}) that carries the unit is
# NOT in OE-core's SYSTEMD_PACKAGES, so the recipe SYSTEMD_AUTO_ENABLE/preset machinery never
# touches it; and a prior `systemctl --root enable` attempt failed outright (exit 1). So enable it
# DETERMINISTICALLY by writing the WantedBy symlink directly — no dependency on preset resolution or
# systemctl. If the unit is absent (PACKAGECONFIG:pn-systemd 'sysext' not effective / stale systemd
# sstate), warn LOUDLY — the extension would otherwise silently never merge at boot.
ROOTFS_POSTPROCESS_COMMAND += "sentient_enable_sysext_service"
sentient_enable_sysext_service() {
    unit="${IMAGE_ROOTFS}${systemd_system_unitdir}/systemd-sysext.service"
    if [ ! -e "$unit" ]; then
        bbwarn "systemd-sysext.service not in the rootfs — PACKAGECONFIG:pn-systemd 'sysext' did not take (stale systemd sstate?); the app extension will NOT merge at boot."
        return 0
    fi
    install -d ${IMAGE_ROOTFS}${sysconfdir}/systemd/system/sysinit.target.wants
    ln -sf ${systemd_system_unitdir}/systemd-sysext.service \
        ${IMAGE_ROOTFS}${sysconfdir}/systemd/system/sysinit.target.wants/systemd-sysext.service
    bbnote "enabled systemd-sysext.service via sysinit.target.wants symlink"
}

# --- overlay/squashfs for the sysext merge — built INTO the kernel (not modules) ---------
# systemd-sysext overlays each extension image onto /usr via the `overlay` filesystem; our
# extension images are `squashfs` (see sentient-app-sysext.bb). We force both built-in (=y) via
# a kernel config fragment (recipes-kernel/linux/files/sysext-fs.cfg, applied to both
# linux-raspberrypi and linux-yocto) rather than installing kernel-module-* packages, because:
#   1) those package names are kernel/config-specific and DON'T EXIST on qemuarm64 (linux-yocto),
#      where these are already built in — installing them broke do_rootfs with
#      "Unable to find a match: kernel-module-overlay kernel-module-squashfs";
#   2) built-in guarantees the filesystems are present at the EARLY boot point
#      systemd-sysext.service merges, with no module-load ordering to get right.
# See sysext-fs.cfg + the linux-*_%.bbappend files for the actual config.

# --- Image minimization (the biggest attack-surface win; compensating control for
#     leaving Zephyr — see CLAUDE.md Phase 1H) -----------------------------------------
# debug-tweaks (on by DEFAULT in core-image-minimal via poky's EXTRA_IMAGE_FEATURES) is
# what enables passwordless root login / empty root password. We do NOT add it, and we
# do not add any of package-management, ssh-server-* (dropbear/openssh), dev-pkgs,
# tools-debug, or tools-sdk. IMAGE_FEATURES below is therefore the COMPLETE feature list
# for this image; nothing else is layered on top.
IMAGE_FEATURES = "read-only-rootfs"

# TODO(verify-at-build): confirm the exact EXTRA_USERS_PARAMS / extrausers.bbclass syntax
# for locking the root account outright (e.g. `usermod -L root` equivalent expressed as an
# EXTRA_USERS_PARAMS entry) against the pinned wrynose poky's
# meta/classes-recipe/extrausers.bbclass — the plan calls this out explicitly as an
# impl-time recipe caveat to confirm from source, not something to guess at here.
inherit extrausers
EXTRA_USERS_PARAMS = "usermod -L root;"

# --- What we deliberately do NOT add -------------------------------------------------
# - package-management: no opkg/rpm/dpkg in the running image (no runtime package install
#   surface; the image ships everything it needs at build time).
# - debug-tweaks: absent -> root has no password and is not auto-logged-in; console login
#   requires the "sentient" service account is not enough on its own, so a real login path
#   should be provisioned deliberately (out of scope for this recipe skeleton) rather than
#   relying on the debug-tweaks default.
# - ssh-server-dropbear / ssh-server-openssh: no remote shell surface baked into the base
#   image; add deliberately per-deployment if field debugging access is required.
# - dev-pkgs / tools-debug / tools-sdk: no compilers/headers/debuggers in the shipped image.

# --- Read-only rootfs implications ----------------------------------------------------
# read-only-rootfs mounts / read-only; systemd's tmpfiles/StateDirectory machinery (see
# sentient-agent.service's StateDirectory=sentient) provides the one writable path the
# agent needs under /var, which along with /var/volatile and friends is handled by
# core-image-minimal's existing read-only-rootfs support (volatile-binds). This is also
# the documented prerequisite for the Phase 3 dm-verity follow-up (CLAUDE.md).

# --- Machines this image targets -------------------------------------------------------
# ARM ONLY (x86 is not a supported platform). Shipping: raspberrypi4-64, raspberrypi5
# (meta-raspberrypi, both aarch64 -> one bin-rpi musl-static build serves both).
# Emulation/CI: qemuarm64 (faithful 64-bit Pi stand-in — bare raspberrypi5 images are not
# runqemu-bootable). MACHINE selection itself is a builder/CI concern (bitbake -e /
# local.conf), not baked into this recipe.

COMPATIBLE_MACHINE = "raspberrypi4-64|raspberrypi5|qemuarm64|qemuarm"

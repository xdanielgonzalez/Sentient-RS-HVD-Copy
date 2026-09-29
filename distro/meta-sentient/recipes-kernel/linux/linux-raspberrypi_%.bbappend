# Sentient METEOR — KSPP kernel hardening fragment for the shipping Yocto Linux image.
#
# The Pi's Yocto image is the compensating control for leaving Zephyr's minimal attack surface
# (see CLAUDE.md "Platform & Deployment" + story #159 / plan phase 1H). This appends a
# hand-written KSPP (Kernel Self Protection Project, https://kspp.github.io/) config fragment to
# whichever `linux-yocto` recipe the distro/machine selects, rather than relying on the upstream
# `meta/recipes-kernel/linux/linux-yocto/features/security` fragment — that in-tree fragment is
# last touched in the 2018 kernel-security-hardening series and does not reflect current KSPP
# recommendations (INIT_ON_ALLOC, RANDOMIZE_KSTACK_OFFSET, LOCKDOWN, etc. postdate it).
#
# TODO(verify-at-build): confirm this bbappend actually matches the recipe selected for the pinned
# wrynose kernel (raspberrypi5/raspberrypi4-64 use `linux-raspberrypi` from meta-raspberrypi in
# some configurations rather than bare `linux-yocto` — if so, this file needs to become/also be a
# `linux-raspberrypi_%.bbappend`, or the machine's PREFERRED_PROVIDER for `virtual/kernel` needs
# confirming against `distro/meta-sentient/conf/distro/sentient.conf` / machine confs owned by
# another workstream). The `%` wildcard matches any recipe version; keep it rather than pinning an
# exact PV so kernel version bumps don't silently stop applying this fragment.

FILESEXTRAPATHS:prepend := "${THISDIR}/files:"

SRC_URI += "file://kspp-hardening.cfg"

# overlay + squashfs built-in, for the systemd-sysext app-extension merge (see sysext-fs.cfg
# and sentient-image.bb). Shared with the qemuarm64 kernel via linux-yocto_%.bbappend.
SRC_URI += "file://sysext-fs.cfg"

# TODO(verify-at-build): `linux-yocto`'s kernel-yocto.bbclass merges *.cfg fragments named in
# SRC_URI automatically via `do_kernel_configcheck` (KCONF_BSP_AUDIT / merge_config.sh under the
# hood). If the recipe in use is `linux-raspberrypi` (not `linux-yocto`), confirm it still inherits
# `kernel-yocto` (it does, historically) so this SRC_URI-fragment mechanism applies unchanged.

# Sentient METEOR — kernel config for the qemuarm64 emulation image (linux-yocto).
#
# qemuarm64 (the CI/UAT emulation-proxy MACHINE) uses poky's `linux-yocto` kernel. This applies
# the sysext filesystem fragment (overlay + squashfs built-in) so the systemd-sysext app
# extension merges at boot there exactly as it will on the shipping Pi kernels — UAT boot-tests
# the real merge, so the emulation kernel must carry the same filesystem support.
#
# NOTE: this is deliberately NARROW — only the sysext-fs fragment. The KSPP hardening fragment is
# applied to the SHIPPING kernel (linux-raspberrypi_%.bbappend), not the emulation kernel; adding
# distro-wide hardening to qemuarm64 is out of scope for this file (track separately if wanted).
# linux-yocto inherits kernel-yocto.bbclass, which merges SRC_URI *.cfg fragments via merge_config.

FILESEXTRAPATHS:prepend := "${THISDIR}/files:"

SRC_URI += "file://sysext-fs.cfg"

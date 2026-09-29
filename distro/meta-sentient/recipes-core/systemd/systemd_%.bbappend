# Build systemd with the `sysext` PACKAGECONFIG (off by default in oe-core wrynose) so the image
# ships the `systemd-sysext` CLI + `systemd-sysext.service` — units/meson.build gates both behind
# ENABLE_SYSEXT, which is driven by PACKAGECONFIG[sysext] = "-Dsysext=true,-Dsysext=false". Without
# this the unit is never built, so the base/app sysext extension can never merge at boot.
#
# This MUST live in a systemd bbappend (or a global .conf), NOT in the image recipe. A
# `PACKAGECONFIG:append:pn-systemd` set in sentient-image.bb is a NO-OP: recipe-scoped `:pn-<name>`
# variable overrides do not cross recipe boundaries into the systemd recipe's own parse — so the
# earlier image-recipe placement never took, and every booted image lacked systemd-sysext.service
# (UAT do_rootfs warning: "systemd-sysext.service not in the rootfs"). See
# recipes-sentient/sentient-image/sentient-image.bb for the base/app split rationale and the
# boot-time enablement (the sysinit.target.wants symlink).
PACKAGECONFIG:append = " sysext"

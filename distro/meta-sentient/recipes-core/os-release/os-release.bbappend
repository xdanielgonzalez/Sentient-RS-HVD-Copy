# os-release.bbappend — adds SYSEXT_LEVEL to the base image's /etc/os-release.
#
# NOTE: the filename is os-release.bbappend, NOT os-release_%.bbappend — oe-core's os-release
# recipe is UNVERSIONED (meta/recipes-core/os-release/os-release.bb, no _<version> in the
# filename), so a `_%` version-wildcard append matches no recipe and BitBake aborts ("No recipes
# in default available for: .../os-release_%.bbappend"). Versioned recipes (mosquitto, base-files)
# do take the `_%` form; os-release does not.
#
# --- Why -----------------------------------------------------------------------------------
# systemd-sysext matches an extension-release fragment against the base OS's os-release by
# ID (always), then by SYSEXT_LEVEL if present on BOTH sides (else it falls back to
# VERSION_ID). We want the SYSEXT_LEVEL match (see recipes-sentient/sentient-app-sysext/
# sentient-app-sysext.bb's header comment for the full contract) so app-sysext builds are
# decoupled from base-OS DISTRO_VERSION bumps — a base security-patch respin does not force
# a rebuild of the app extension, and vice versa, as long as SYSEXT_LEVEL (a stable ABI
# counter, not a version) is unchanged.
#
# ID/VERSION_ID on the base image are left exactly as poky/oe-core's stock os-release
# recipe already derives them from DISTRO/DISTRO_VERSION (see conf/distro/sentient.conf) —
# this bbappend adds ONLY the new SYSEXT_LEVEL field, nothing else.
#
# Frozen contract value: SYSEXT_LEVEL=1 (bump only on an intentional app/base ABI break —
# e.g. a change to where the sysext expects to find files under /usr, or to the
# unit/user/config contract sentient-agent.service + the "sentient" user assume).
#
# TODO(verify-at-build): oe-core's recipes-core/os-release/os-release.bb (wrynose) is
# believed to support extra fields via its OS_RELEASE_FIELDS list (append the field name
# there, then set the value as its own variable) rather than a raw do_install:append text
# splice — confirm the exact mechanism against the pinned checkout
# (`bitbake -e os-release | grep -i OS_RELEASE_FIELDS`) and switch to that form if so; the
# do_install:append below is a defensive fallback that works regardless (appends a line to
# the already-generated /etc/os-release) but duplicates the field if the class-native
# mechanism is also used, so pick ONE approach once this is verified.

do_install:append() {
    if ! grep -q '^SYSEXT_LEVEL=' ${D}${sysconfdir}/os-release 2>/dev/null; then
        echo 'SYSEXT_LEVEL=1' >> ${D}${sysconfdir}/os-release
    fi
}

# Sentient METEOR — branded console/login banner.
#
# Installs our /etc/issue (local console) and /etc/issue.net (remote/network login banner —
# ships even though the image has no ssh-server by default per image-minimization, in case a
# deployment deliberately re-adds one) over the stock base-files versions. This is text-banner
# branding only — OS identity (os-release DISTRO_NAME/DISTRO_VERSION/VERSION_ID) is owned by
# another workstream's custom DISTRO conf (`distro/meta-sentient/conf/distro/sentient.conf`,
# story #159 phase 1) and intentionally NOT touched here.

FILESEXTRAPATHS:prepend := "${THISDIR}/files:"

SRC_URI += "file://issue file://issue.net"

do_install:append() {
    install -m 0644 ${UNPACKDIR}/issue     ${D}${sysconfdir}/issue
    install -m 0644 ${UNPACKDIR}/issue.net ${D}${sysconfdir}/issue.net
}

# TODO(verify-at-build): confirm base-files' own do_install doesn't run *after* this append and
# clobber these two files back to stock content — base-files traditionally installs /etc/issue*
# from its own template in the same do_install, so a plain do_install:append (running after the
# base task body) should win, but confirm against the pinned wrynose base-files recipe's task
# ordering rather than assuming; if it does NOT win, switch to a do_install_append that runs the
# stock install first then overwrites (already the case here) or override FILESPATH searching
# instead so base-files' own SRC_URI picks up our files under that name.

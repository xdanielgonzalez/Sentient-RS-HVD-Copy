# Sentient METEOR — default-drop firewall ruleset for the shipping Yocto Linux image.
#
# Installs a static nftables ruleset that ships with the `nftables` package (from
# meta-networking) and enables its systemd service so the ruleset is loaded at boot with no
# operator step. This is the network-hardening leg of story #159 phase 1H item 4.
#
# Architecture note (see docs/deployment.md + docs/security.md — read before changing the
# ruleset): `bin-rpi`'s embedded rumqttd broker binds `127.0.0.1:1883` LOOPBACK-ONLY by design
# (unauthenticated plaintext; docs/security.md §1.1-§1.2). The only network-facing MQTT traffic
# this device originates is the mosquitto bridge's OUTBOUND connection to the private-cloud
# endpoint on TLS 8883 (docs/deployment.md §5) — nothing on this box needs to ACCEPT an inbound
# MQTT connection in the default configuration. `sentient.nft` therefore ships a default-drop
# INPUT policy with no inbound MQTT ACCEPT rule enabled by default; a commented-out rule is
# provided for the deliberate, documented case where an operator widens `[broker] host` off
# loopback (docs/deployment.md §4's "Broker bind is loopback by design" callout) and needs to
# expose it to a specific LAN subnet.
#
# TODO(verify-at-build): confirm the exact recipe name/version nftables ships as under the pinned
# wrynose meta-networking layer (this file assumes `nftables_%.bbappend` matches; if
# meta-networking instead names it e.g. `nftables_1.0.9.bb`, the `%` wildcard still matches, but
# confirm FILESEXTRAPATHS resolves against the recipe's own file search path, not a different
# recipe entirely).

FILESEXTRAPATHS:prepend := "${THISDIR}/files:"

SRC_URI += "file://sentient.nft"

# TODO(verify-at-build): confirm the exact install path the upstream nftables recipe expects for
# its default ruleset. Debian/most distros use /etc/nftables.conf as the file the systemd unit's
# ExecStart references (`nft -f /etc/nftables.conf`); the Yocto nftables recipe from
# meta-networking may instead expect /etc/nftables/ or a different default ruleset filename —
# check `nftables.service`'s ExecStart in the recipe's packaged unit file before assuming
# /etc/nftables.conf is correct. Adjust the destination below if it differs.
do_install:append() {
    install -d ${D}${sysconfdir}
    install -m 0600 ${UNPACKDIR}/sentient.nft ${D}${sysconfdir}/nftables.conf
}

FILES:${PN} += "${sysconfdir}/nftables.conf"

# Enable the nftables systemd service unconditionally so the ruleset loads at boot with no
# first-boot manual step (image minimization already drops ssh-server/package-management, but the
# firewall is a belt-and-suspenders control against whatever else ends up listening).
inherit systemd
SYSTEMD_SERVICE:${PN} += "nftables.service"
SYSTEMD_AUTO_ENABLE:${PN} = "enable"
# TODO(verify-at-build): confirm the packaged systemd unit is literally named `nftables.service`
# in the meta-networking recipe (some distros ship `nftables.service` templated per ruleset file,
# e.g. `nftables@.service` — if so this line needs the instance suffix instead).

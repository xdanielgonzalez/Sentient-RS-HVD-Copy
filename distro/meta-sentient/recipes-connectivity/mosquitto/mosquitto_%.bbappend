# mosquitto_%.bbappend — installs the Sentient METEOR cloud-relay bridge config on top of
# the upstream mosquitto recipe (meta-openembedded/meta-networking). The "%" wildcard
# matches whatever mosquitto PV the pinned meta-networking ships at wrynose, so this
# bbappend does not need updating on routine mosquitto version bumps.
#
# This is the Yocto-image counterpart of distro/deploy/mosquitto.conf +
# distro/deploy/mosquitto-override.conf (see docs/deployment.md §5 for the full runbook this
# packages). Behaviour is intentionally identical to that manual-deploy path:
#   - the embedded rumqttd broker in bin-rpi listens on 127.0.0.1:1883 (loopback only);
#   - THIS mosquitto instance bridges tms/# IN from that loopback broker and back OUT to
#     the private-cloud endpoint over TLS;
#   - mosquitto 2.x cannot expand ${...} env vars in its own config, so the cloud-relay
#     bridge block (real host/port/username/ca_file + the CLOUD_MQTT_PASS secret) is
#     rendered at service-start time by `bin-rpi --emit-mosquitto-bridge`, run as an
#     ExecStartPre via the systemd drop-in below — never baked into the image as a static
#     file with placeholders.
#
# TODO(verify-at-build): confirm the exact upstream mosquitto recipe name/path this
# bbappend targets (meta-openembedded/meta-networking/recipes-connectivity/mosquitto/
# mosquitto_*.bb as of wrynose) and that its systemd unit is named `mosquitto.service`
# (used below for the override.conf drop-in directory name) — these are standard
# meta-networking conventions but should be checked against the pinned layer checkout.

FILESEXTRAPATHS:prepend := "${THISDIR}/files:"

SRC_URI += "file://sentient.conf \
            file://mosquitto-override.conf"

do_install:append() {
    # Static bridge config: device-broker (loopback, in-image) side only. The cloud-relay
    # block is intentionally NOT here — it has no static form, see header comment above.
    install -d ${D}${sysconfdir}/mosquitto/conf.d
    install -m 0644 ${UNPACKDIR}/sentient.conf ${D}${sysconfdir}/mosquitto/conf.d/sentient.conf

    # CRITICAL (UAT run 29433182716): the stock meta-networking mosquitto_2.1.2 recipe installs
    # upstream's ${S}/mosquitto.conf verbatim, which ships `include_dir` COMMENTED OUT and adds no
    # conf.d include of its own. So WITHOUT this, mosquitto never reads conf.d/sentient.conf at all —
    # it silently ignores BOTH our device-broker bridge AND the listener-suppression, and falls back
    # to its default localhost:1883 listener, which then collides with bin-rpi's embedded rumqttd
    # (EADDRINUSE) and breaks the device broker + the whole relay. Make mosquitto actually load
    # conf.d. Idempotent guard so a future upstream that DOES add the include never double-loads it
    # (a duplicate include_dir would parse sentient.conf twice -> duplicate `listener`/`connection`
    # -> startup error).
    if ! grep -qE "^[[:space:]]*include_dir[[:space:]]+${sysconfdir}/mosquitto/conf.d[[:space:]]*$" \
            ${D}${sysconfdir}/mosquitto/mosquitto.conf; then
        printf '\n# Sentient METEOR: load /etc/mosquitto/conf.d/*.conf (device-broker bridge +\n# listener-suppression). Upstream ships include_dir commented; add it explicitly.\ninclude_dir %s/mosquitto/conf.d\n' \
            "${sysconfdir}" >> ${D}${sysconfdir}/mosquitto/mosquitto.conf
    fi

    # BOTH include_dir directives MUST be in the main config: mosquitto 2.x rejects include_dir
    # inside an included file. This one loads the RENDERED cloud-relay bridge that
    # `bin-rpi --emit-mosquitto-bridge` writes into /etc/sentient/mosquitto.d/ at ExecStartPre
    # (mosquitto-owned so the render step can write it). Empty dir (local-only device) = no-op.
    # It used to live (illegally) in conf.d/sentient.conf, which crash-looped mosquitto
    # (UAT 29434470958) and killed the whole cloud relay.
    if ! grep -qE "^[[:space:]]*include_dir[[:space:]]+${sysconfdir}/sentient/mosquitto.d[[:space:]]*$" \
            ${D}${sysconfdir}/mosquitto/mosquitto.conf; then
        printf '\n# Sentient METEOR: load the rendered cloud-relay bridge (Greengrass funnel).\ninclude_dir %s/sentient/mosquitto.d\n' \
            "${sysconfdir}" >> ${D}${sysconfdir}/mosquitto/mosquitto.conf
    fi

    # Directory the rendered cloud-relay.conf lands in at service-start time (via
    # bin-rpi --emit-mosquitto-bridge, wired below). mosquitto-owned so the ExecStartPre
    # render step (running as the mosquitto service user) can write into it.
    install -d ${D}${sysconfdir}/sentient/mosquitto.d

    # systemd drop-in: wires the render step in as an ExecStartPre + loads the
    # CLOUD_MQTT_PASS secret (if present) before mosquitto starts.
    install -d ${D}${systemd_system_unitdir}/mosquitto.service.d
    install -m 0644 ${UNPACKDIR}/mosquitto-override.conf \
        ${D}${systemd_system_unitdir}/mosquitto.service.d/override.conf
}

FILES:${PN} += "${sysconfdir}/mosquitto/conf.d/sentient.conf \
                ${sysconfdir}/sentient/mosquitto.d \
                ${systemd_system_unitdir}/mosquitto.service.d/override.conf"

# /etc/sentient/mosquitto.d is written into by the ExecStartPre render step at runtime
# (bin-rpi running as the "sentient" user's render path, or mosquitto's own ExecStartPre
# context — see distro/deploy/mosquitto-override.conf's original ownership note); mark it as a
# config-managed writable path so package upgrades don't clobber a rendered bridge file.
CONFFILES:${PN} += "${sysconfdir}/mosquitto/conf.d/sentient.conf"

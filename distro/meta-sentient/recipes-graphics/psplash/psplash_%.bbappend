# Sentient METEOR — branded, versioned boot splash.
#
# psplash is Yocto's boot-splash daemon (gated by the `splash` DISTRO/IMAGE feature +
# `SPLASH = "psplash"` in conf/distro/sentient.conf). This bbappend replaces psplash's compiled-in
# Poky logo with the Sentient splash: the brand mark + "SENTIENT INDUSTRIES" wordmark + the release
# version.
#
# MECHANISM (verified against the pinned meta/recipes-core/psplash/psplash_git.bb, not guessed): the
# stock recipe's `do_compile` iterates ${SPLASH_IMAGES}, runs `make-image-header.sh` on each `.png`,
# and copies the entry tagged `outsuffix=default` into `psplash-poky-img.h` — the image compiled into
# the psplash binary. So overriding SPLASH_IMAGES to our PNG (with `;outsuffix=default`) + shipping
# that PNG via FILESEXTRAPATHS is the ENTIRE change: no do_compile patch, no committed C header. The
# default PACKAGECONFIG includes `fullscreen`, so the framebuffer scales this 16:9 PNG to the display.
#
# VERSIONED / PIPELINE-RENDERED: files/sentient-logo.png is a committed DEFAULT (rendered at the
# distro version). The release pipeline REGENERATES it with the actual release version before the
# image build, via files/gen-splash.py (Cairo/cairosvg): it composes files/mark.png + the wordmark +
# `--version <release>` into the splash. See files/README-logo.md. So the boot screen always shows the
# exact shipped version, and the committed default keeps a plain `kas build` self-contained.

FILESEXTRAPATHS:prepend := "${THISDIR}/files:"

SPLASH_IMAGES = "file://sentient-logo.png;outsuffix=default"

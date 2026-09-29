# security_flags.inc — distro-wide compiler/binary hardening flags.
#
# Included from conf/distro/sentient.conf so every recipe built for this distro (kernel
# excluded — it has its own hardening path, see recipes-kernel/) picks these up: PIE,
# full RELRO + BIND_NOW, stack-protector, and FORTIFY_SOURCE.
#
# Our own deliverable (bin-rpi) is a prebuilt, already-hardened static-PIE Rust binary
# (rustc defaults to PIE + full RELRO on the aarch64-unknown-linux-musl target) and is
# NOT recompiled by BitBake, so these flags don't touch it directly — they harden every
# OTHER C/C++ package in the image (busybox, mosquitto, openssl, the kernel's userspace
# tools, etc.), which is the actual attack surface reduction this file buys.
#
# TODO(verify-at-build): confirm these exact SECURITY_* variable names/values against
# poky's meta/conf/distro/include/security_flags.inc for the pinned wrynose checkout —
# the plan intentionally does not fabricate the recipe-level machinery.

# PIE (position-independent executables) for all recipes.
SECURITY_PIE ??= "-fpie -pie"
SECURITY_PIE:pn-glibc = ""

# Full RELRO + immediate binding (closes the GOT-overwrite window after load).
SECURITY_RELRO ??= "-Wl,-z,relro,-z,now"

# Stack-smashing protection profile. "strong" is the current recommended strength; the
# clean-tag reduces to the (weaker) legacy "-fstack-protector" for callers that opt out.
SECURITY_STACKPROTECTOR ??= "-fstack-protector-strong"
SECURITY_NOPIE_CFLAGS ??= ""
SECURITY_X_LDFLAGS ??= "-Wl,-z,noexecstack"

# FORTIFY_SOURCE requires an optimization level >= -O1; wrynose defaults already satisfy
# this for release builds. -D_FORTIFY_SOURCE=2 adds buffer-overflow checks to common libc
# calls (memcpy/strcpy/sprintf/...) where the compiler can determine object size.
SECURITY_CFLAGS ??= "${SECURITY_STACKPROTECTOR} ${SECURITY_PIE} -D_FORTIFY_SOURCE=2 -Wformat -Wformat-security -Werror=format-security"
SECURITY_LDFLAGS ??= "${SECURITY_RELRO} ${SECURITY_X_LDFLAGS}"

# Wire the fragments into the flags every recipe sees.
TARGET_CC_ARCH:append = " ${SECURITY_CFLAGS}"
TARGET_LDFLAGS:append = " ${SECURITY_LDFLAGS}"

# Residual gap (documented, not fixed here): stable Rust has no direct equivalent to
# -fstack-protector for our own Rust binary (nightly-only `-Z stack-protector`); accepted
# per CLAUDE.md Phase 1H — verify this file's effect on OTHER image packages via
# `readelf -h`/`checksec`, and verify bin-rpi's own PIE/RELRO status separately (it is a
# rustc property, not something this .inc can add to a prebuilt artifact).

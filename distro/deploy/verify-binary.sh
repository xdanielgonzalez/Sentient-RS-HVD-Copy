#!/bin/sh
# verify-binary.sh — refuse to start the service unless the binary matches its X.509 CMS signature.
#
# Tier-1 integrity control: pairs with the systemd unit's ExecStartPre. A tampered or swapped binary
# fails the check and the service does not start. This verifies a DETACHED CMS signature (<bin>.p7s,
# DER) against our self-signed CA — the same signature production.yml's release job attaches
# ("Sign RPi binaries (X.509 CMS)"). Available today without RPi EEPROM signed boot / OTP; see
# docs/security.md for the full boot-integrity tiers.
#
# Signing (CI, offline key):
#   openssl cms -sign -binary -in bin-rpi -signer signing.crt -inkey signing.key \
#       -certfile ca.crt -outform DER -out bin-rpi.p7s
# Deploy to the device (root-owned 0644):
#   sudo install -D -m0644 ca.crt      /etc/sentient/ca.crt
#   sudo install -D -m0644 bin-rpi.p7s /usr/local/bin/bin-rpi.p7s
#
# Exit non-zero blocks ExecStart.
set -eu

BIN="${1:-/usr/local/bin/bin-rpi}"
CAFILE="${SENTIENT_CA:-/etc/sentient/ca.crt}"
SIG="${SENTIENT_SIG:-${BIN}.p7s}"

if ! command -v openssl >/dev/null 2>&1; then
    echo "verify-binary: openssl not installed; cannot verify ${BIN}" >&2
    exit 1
fi
if [ ! -f "$CAFILE" ]; then
    echo "verify-binary: CA cert not found at ${CAFILE}" >&2
    exit 1
fi
if [ ! -f "$SIG" ]; then
    echo "verify-binary: signature not found at ${SIG}" >&2
    exit 1
fi

# Detached CMS verify: -content is the signed file, -in is the .p7s (DER), -CAfile is the trust anchor.
# -binary must match the signer side (no S/MIME canonicalization on the raw ELF bytes).
if openssl cms -verify -binary -content "$BIN" -in "$SIG" -inform DER -CAfile "$CAFILE" -out /dev/null >/dev/null 2>&1; then
    echo "verify-binary: OK ${BIN}"
    exit 0
fi

echo "verify-binary: SIGNATURE VERIFICATION FAILED for ${BIN} — refusing to start" >&2
exit 1

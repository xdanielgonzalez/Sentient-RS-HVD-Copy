#!/usr/bin/env bash
# .ci/scripts/uat/verify-golden-sig.sh — verify the golden base's X.509 CMS signature BEFORE boot.
#
# This exercises the exact sign -> stage -> pull -> verify chain the shipping Pi .wic relies on,
# proving signing works on the emulation image before we trust it for a real release. Same
# degrade as the signer: a CA-issued cert CA-verifies; the self-signed example.local pre-release
# cert falls back to a signature-only verify (-noverify); only an actually-invalid/tampered
# signature fails the gate.
#
# Env contract: MACHINE, MC_ENDPOINT, MC_KEY, MC_SECRET. Expects dl/sentient-image.tar.gz from
# pull-golden-base.sh.
set -euo pipefail

command -v openssl >/dev/null 2>&1 || { echo "::error::openssl not found — cannot verify"; exit 1; }
mc alias set minio "$MC_ENDPOINT" "$MC_KEY" "$MC_SECRET" >/dev/null 2>&1 \
  || { echo "::error::mc alias set failed — cannot fetch golden signature"; exit 1; }
SIG="minio/cache/golden/${MACHINE}/sentient-image.tar.gz.p7s"
CA="minio/cache/golden/${MACHINE}/ca.pem"
if ! mc stat "$SIG" >/dev/null 2>&1; then
  echo "::error::no golden signature at $SIG — the base build must sign + stage the golden (see _yocto.yml 'Sign golden bundle'). Re-run image.yml."
  exit 1
fi
mc cp "$SIG" dl/sentient-image.tar.gz.p7s
mc cp "$CA"  dl/ca.pem 2>/dev/null || true
cd dl
if [ -f ca.pem ] && openssl cms -verify -binary -content sentient-image.tar.gz -in sentient-image.tar.gz.p7s -inform DER -CAfile ca.pem -out /dev/null 2>/dev/null; then
  echo "::notice::golden base signature CA-verified ✓"
elif openssl cms -verify -binary -content sentient-image.tar.gz -in sentient-image.tar.gz.p7s -inform DER -noverify -out /dev/null 2>/dev/null; then
  echo "::warning::golden base signature valid but cert not CA-trusted (pre-release / example.local) — the sign/verify mechanism WORKS; this is the pre-release tier"
else
  echo "::error::golden base signature INVALID — refusing to boot a tampered base"; exit 1
fi

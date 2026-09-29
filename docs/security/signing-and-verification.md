# Signing and verification

Release artifacts for `sentient-rs` carry **two independent, non-overlapping signature
mechanisms**. Do not conflate them — they answer different questions and are verified in
different places:

| Mechanism | Signs | Verified by | Answers |
|---|---|---|---|
| **cosign (keyless / Sigstore)** | `SHA256SUMS` (the aggregate checksum manifest) | a human or CI, on demand, off-device | "Was this release published by the `sentient-rs` GitHub Actions release workflow, and is `SHA256SUMS` exactly what it published?" |
| **X.509 CMS (key-based)** | each runnable `bin-rpi-<slug>` binary and each `sentient-image-*.wic`/`.wic.bz2` individually | the device itself, at every service start (`distro/deploy/verify-binary.sh`, via `ExecStartPre`); and anyone, off-device, against the attached CA | "Is the artifact on disk right now byte-for-byte what was signed by the offline release key, and does it chain to our CA?" |

cosign gives **release provenance** (a public, auditable attestation tied to the CI identity that
built it). X.509 CMS gives **on-device boot-time integrity** (a pre-start gate that blocks a
tampered or swapped binary from ever running) plus off-device artifact authenticity. Both are
defined in `.github/workflows/production.yml` (the `release`
and `sysext` jobs); the on-device gate is
`distro/deploy/verify-binary.sh`. See [`security.md`](./README.md) §2 for
how the CMS gate fits into the broader RPi boot-integrity tiering.

> **Correcting a common misconception:** the cosign bundle is **not a key you create ahead of
> time**. It records a **short-lived signing certificate** issued by Sigstore's Fulcio CA at sign
> time, bound to the CI job's OIDC identity, and valid for minutes. There is no long-lived cosign
> private key to generate, store, or rotate — that is the entire point of keyless signing. Contrast
> this with X.509 CMS, where you *do* generate and hold a real long-lived private key + certificate
> issued by a self-signed CA (§4 below).

## 1. Overview: where each step happens

Both signing steps live in the `release` and `yocto` jobs of
`.github/workflows/production.yml`, which run only when a `v*.*.*` tag is pushed
(`if: startsWith(github.ref, 'refs/tags/v')`):

1. **`Sign RPi binaries (X.509 CMS)`** — for each runnable stripped `bin-rpi-<slug>` binary (not the
   `.elf`/`.tar.gz`/`.sha256`/`.sbom.json`/`.p7s`/`.pem` siblings), runs
   `openssl cms -sign -binary -in "$bin" -signer signing.crt -inkey signing.key -certfile ca.crt
   -outform DER -out "$bin.p7s"` using the `SIGNING_CRT` (signer cert), `SIGNING_KEY` (signer
   private key), and `CA_CRT` (CA trust anchor) repo secrets. **If any of the three is unset, this
   step prints a warning and skips rather than failing the release** — cosign provenance still
   applies even when X.509 signing is not configured. The `yocto` job runs the same command over
   each `sentient-image-*.wic`/`.wic.bz2`.
2. **`Sign checksums with cosign (keyless)`** — `cosign sign-blob --yes --bundle
   SHA256SUMS.cosign.bundle SHA256SUMS`. Uses the job's ambient GitHub Actions OIDC token
   (`permissions: id-token: write` at the job level) — no secret material at all. (cosign 3.x
   dropped the detached `--output-certificate`/`--output-signature` path; the modern form emits a
   single self-describing `.cosign.bundle`.)
3. **`Create release`** (`softprops/action-gh-release`) attaches, per the `files:` list:
   `bin-rpi-*` (binary + `.elf` + `.tar.gz` + `.sbom.json`), `bin-zephyr-*`, the signed
   `.wic`/`.wic.bz2`, `SHA256SUMS`, every `.p7s` detached CMS signature, `signing-cert.pem` (the
   signer cert), `ca.pem` (the CA), and `SHA256SUMS.cosign.bundle`.

Because both signing steps are resilient-by-design (skip-on-missing-secret for CMS; the cosign step
must not abort the release either), **a release can legitimately ship with no signatures at all** if
secrets are absent or Sigstore is unreachable — `SHA256SUMS` alone (plain checksums, no signature)
is always present. Always check what actually landed on a given release (§6) rather than assuming
both mechanisms fired.

> **Known gap — v0.2.0 (issue [#105](https://github.com/High-Velocity-Decisions/sentient-rs/issues/105)):**
> the `softprops/action-gh-release` asset-metadata update hit a transient 404 mid-upload and
> aborted the attach step. All build assets (binaries, `.elf`, `.tar.gz`, `.sbom.json`,
> `SHA256SUMS`) made it onto the release, but the cosign provenance and the detached `.p7s`
> signatures (plus `signing-cert.pem` / `ca.pem`) were dropped. The release itself was also left in
> `draft` state and had to be published manually. §2.2 below shows how to regenerate and re-attach
> the missing cosign provenance for an already-tagged release. The `.p7s` signatures cannot be
> regenerated after the fact by a third party — only whoever holds `SIGNING_KEY` can produce those
> (§4/§5), and rotating the release binaries afterward is not equivalent to re-signing the original
> artifact.

## 2. cosign — creating `SHA256SUMS.cosign.bundle`

### 2.1 How CI does it (ambient OIDC, no secrets)

Nothing to configure beyond `permissions: id-token: write` at the job level (already set in
`production.yml`). Every tag push automatically runs:

```sh
cosign sign-blob --yes \
  --bundle SHA256SUMS.cosign.bundle \
  SHA256SUMS
```

`--yes` skips the interactive confirmation prompt (non-interactive CI context). GitHub Actions
injects the OIDC token; cosign exchanges it with Sigstore's Fulcio CA for a short-lived signing
certificate (see §2.3), signs `SHA256SUMS` with the matching ephemeral key, records the transaction
in the public Rekor transparency log, and writes the self-describing bundle (certificate +
signature + Rekor inclusion proof in one file). No secret is read or stored anywhere in this path.

### 2.2 How to do it manually (re-attaching the missing v0.2.0 provenance)

You need the exact `SHA256SUMS` bytes that were published for that release — cosign signs the file
content, so re-generating `SHA256SUMS` from scratch must reproduce it byte-for-byte, or download
the one already attached to the release instead of rebuilding it:

```sh
# Fetch the actual published SHA256SUMS (do not regenerate it locally — sign exactly what shipped).
gh release download v0.2.0 --repo High-Velocity-Decisions/sentient-rs \
  --pattern 'SHA256SUMS' --dir /tmp/v0.2.0
cd /tmp/v0.2.0

# Interactive OIDC login: cosign opens a browser, you authenticate with your identity provider
# (GitHub/Google/Microsoft — whichever Sigstore's Fulcio accepts), and it mints a short-lived
# certificate binding the bundle to *your* identity, not the CI workflow's identity.
cosign sign-blob --yes \
  --bundle SHA256SUMS.cosign.bundle \
  SHA256SUMS

# Attach to the existing release.
gh release upload v0.2.0 SHA256SUMS.cosign.bundle --repo High-Velocity-Decisions/sentient-rs
```

**Important caveat:** a manually-produced signature is bound to *your* personal OIDC identity
(e.g. `you@example.com` via GitHub or Google), not to
`.github/workflows/production.yml@refs/tags/v0.2.0`. Recipients verifying with the CI-workflow
identity regexp from §3 will **not** match a manually-signed cert — you would need to give them
your own identity/issuer to verify against instead, and that is a materially weaker claim ("a
maintainer attests this" vs. "the release CI pipeline produced this from this exact tag"). The
durable fix for #105 is making the CI attach step reliable (pin/replace the release action, or add
upload retry), not manual re-signing after the fact — treat this recipe as a stopgap for the
already-tagged v0.2.0, not the steady state.

### 2.3 Where the bundle's certificate comes from

Fulcio (Sigstore's certificate authority) issues the signing certificate recorded inside
`SHA256SUMS.cosign.bundle` at sign time. It embeds the verified OIDC identity (for CI: the GitHub
Actions workflow path + git ref; for a human: their IdP-verified email) as a certificate extension,
and is valid for a few minutes — long enough to complete the signing operation, never intended to be
reused or stored as a "key." The corresponding private key is generated in-memory by cosign for that
single signing operation and discarded immediately after; it is never written to disk. This is why
there is no cert/key file to provision ahead of time, and no rotation story — a fresh cert is minted
on every single `sign-blob` call.

## 3. cosign — verifying `SHA256SUMS`

Verification needs the **exact CI identity** that would have produced the certificate. For this
repo's release job, the workflow file is `.github/workflows/production.yml` and it only runs the
signing steps on a tag push (`refs/tags/v*`), so the certificate identity Fulcio embeds is:

```
https://github.com/High-Velocity-Decisions/sentient-rs/.github/workflows/production.yml@refs/tags/v<version>
```

Use a regex to match any tag rather than hardcoding one version:

```sh
COSIGN_IDENTITY_REGEXP='^https://github\.com/High-Velocity-Decisions/sentient-rs/\.github/workflows/production\.yml@refs/tags/v.*$'
COSIGN_OIDC_ISSUER='https://token.actions.githubusercontent.com'
```

### 3.1 Passing example

```sh
mkdir -p /tmp/verify && cd /tmp/verify
gh release download v0.2.0 --repo High-Velocity-Decisions/sentient-rs \
  --pattern 'SHA256SUMS*'

cosign verify-blob \
  --bundle SHA256SUMS.cosign.bundle \
  --certificate-identity-regexp "$COSIGN_IDENTITY_REGEXP" \
  --certificate-oidc-issuer "$COSIGN_OIDC_ISSUER" \
  SHA256SUMS
```

Expected output ends with:

```
Verified OK
```

(If you are verifying v0.2.0 specifically: this will currently fail with a missing-file error, not
a signature-mismatch error — `SHA256SUMS.cosign.bundle` is not on that release yet, per the #105 gap
in §1. It will succeed once §2.2's re-attach recipe has been run, or on any release cut after
`production.yml`'s cosign step runs cleanly.)

### 3.2 Tampered-file failing example

```sh
# Flip one byte in a local copy of SHA256SUMS and re-verify against the original signature.
cp SHA256SUMS SHA256SUMS.tampered
printf 'x' | dd of=SHA256SUMS.tampered bs=1 seek=0 count=1 conv=notrunc 2>/dev/null

cosign verify-blob \
  --bundle SHA256SUMS.cosign.bundle \
  --certificate-identity-regexp "$COSIGN_IDENTITY_REGEXP" \
  --certificate-oidc-issuer "$COSIGN_OIDC_ISSUER" \
  SHA256SUMS.tampered
```

Expected output (fails closed):

```
Error: verifying blob [...]: signature does not match digest
```

A wrong `--certificate-identity-regexp` (e.g. pointing at a fork or a different workflow file)
fails the same way — closed, not open — with `none of the expected identities matched`.

## 4. X.509 CMS — key/cert creation and CI secrets

### 4.1 Generate the CA and signer keypair (once, offline)

Signing uses a **self-signed CA** as the trust anchor plus a **signer certificate/key** issued by
that CA. Generate both on an offline or otherwise trusted machine — the signer key is the release
repo's only key-based signing secret; anyone holding it can produce artifacts the device will trust.

```sh
# 1. Self-signed CA (the trust anchor devices and verifiers pin).
openssl req -x509 -newkey rsa:4096 -keyout ca.key -out ca.crt \
  -days 3650 -nodes -subj "/CN=sentient-rs signing CA"

# 2. Signer key + CSR, then sign it with the CA to get the leaf signer cert.
openssl req -newkey rsa:4096 -keyout signing.key -out signing.csr \
  -nodes -subj "/CN=sentient-rs release signer"
openssl x509 -req -in signing.csr -CA ca.crt -CAkey ca.key -CAcreateserial \
  -days 825 -out signing.crt
```

- `ca.crt` — the CA certificate (public — safe to commit/distribute; it is the trust anchor).
- `signing.crt` — the signer certificate (public — attached to releases as `signing-cert.pem`).
- `ca.key` / `signing.key` — private keys (**never commit these**). Keep `ca.key` in the most
  secure offline custody; `signing.key` is what CI needs.

### 4.2 Load the three repo secrets CI needs

`production.yml`'s "Sign RPi binaries (X.509 CMS)" step (and the `yocto` job's `.wic` signing step)
read three secrets:

- **`SIGNING_CRT`** — the full PEM contents of `signing.crt` (the signer certificate).
- **`SIGNING_KEY`** — the full PEM contents of `signing.key` (the signer private key).
- **`CA_CRT`** — the full PEM contents of `ca.crt` (the CA trust anchor).

Set all three as **repository secrets** (Settings → Secrets and variables → Actions → New repository
secret) on `High-Velocity-Decisions/sentient-rs`:

```sh
gh secret set SIGNING_CRT --repo High-Velocity-Decisions/sentient-rs < signing.crt
gh secret set SIGNING_KEY --repo High-Velocity-Decisions/sentient-rs < signing.key
gh secret set CA_CRT      --repo High-Velocity-Decisions/sentient-rs < ca.crt
```

If any of the three is absent, the signing step prints a warning and skips (its own guard:
`if [ -z "${SIGNING_KEY:-}" ] || [ -z "${SIGNING_CRT:-}" ] || [ -z "${CA_CRT:-}" ]; then …`) —
cosign provenance still applies. Setting all three is sufficient to turn per-artifact signing on for
the next tagged release; no workflow change is needed.

### 4.3 Distribute the CA to the device

`ca.crt` is not a secret. Ship it with the release (attached as `ca.pem`) and install it on every
device that will run `bin-rpi`, root-owned and world-readable:

```sh
sudo install -D -m0644 ca.pem /etc/sentient/ca.crt
```

This is the exact path `distro/deploy/verify-binary.sh` reads by default (`SENTIENT_CA`, defaulting
to `/etc/sentient/ca.crt`) — see [`deployment.md`](../deployment/README.md) for the full device install
sequence.

## 5. X.509 CMS — signing and verifying

### 5.1 How the release signs each artifact

For every runnable stripped `bin-rpi-<slug>` binary (explicitly *not* the `.elf`, `.tar.gz`,
`.sha256`, `.sbom.json`, `.p7s`, or `.pem` files that share the `bin-rpi-*` glob), and — in the
`yocto` job — every `sentient-image-*.wic`/`.wic.bz2`:

```sh
openssl cms -sign -binary -in "$FILE" -signer signing.crt -inkey signing.key \
  -certfile ca.crt -outform DER -out "$FILE.p7s"
```

This produces a **detached** CMS signature file, `<FILE>.p7s` (DER-encoded), next to the artifact.
`production.yml` attaches these via the `files: artifacts/*.p7s` line, along with `signing-cert.pem`
(the signer cert) and `ca.pem` (the CA), in the `Create release` step. `-binary` is load-bearing: it
signs the raw bytes with no S/MIME canonicalization, so the verify side must match it.

### 5.2 How the device verifies (the boot-time gate)

`distro/deploy/verify-binary.sh` is invoked by `distro/deploy/bin-rpi.service` as
`ExecStartPre=/usr/local/bin/verify-binary.sh /usr/local/bin/bin-rpi`. A non-zero exit blocks
`ExecStart` — systemd will not launch the service. Its core check:

```sh
openssl cms -verify -binary -content "$BIN" -in "$SIG" -inform DER -CAfile "$CAFILE"
```

where (by default) `BIN=/usr/local/bin/bin-rpi`, `CAFILE=/etc/sentient/ca.crt`, and
`SIG=${BIN}.p7s`. The CA file and signature path are overridable via `SENTIENT_CA` / `SENTIENT_SIG`
env vars, or by passing a different `BIN` as `$1`.

### 5.3 Passing example (manual, off-device)

```sh
gh release download v0.2.0 --repo High-Velocity-Decisions/sentient-rs \
  --pattern 'bin-rpi-linux-x86_64*' --pattern 'ca.pem'

openssl cms -verify -binary -content bin-rpi-linux-x86_64 \
  -in bin-rpi-linux-x86_64.p7s -inform DER -CAfile ca.pem
```

Expected output (`openssl` prints to stderr on success):

```
CMS Verification successful
```

(As with cosign in §3.1: this specific v0.2.0 asset set has no `.p7s` files yet, per the #105 gap —
the command above will report a missing file until either the secrets are set for a future release,
or the signing key holder produces `.p7s` files for v0.2.0 out of band.)

### 5.4 Tampered-file failing example

```sh
cp bin-rpi-linux-x86_64 bin-rpi-linux-x86_64.tampered
printf 'x' | dd of=bin-rpi-linux-x86_64.tampered bs=1 seek=0 count=1 conv=notrunc 2>/dev/null

openssl cms -verify -binary -content bin-rpi-linux-x86_64.tampered \
  -in bin-rpi-linux-x86_64.p7s -inform DER -CAfile ca.pem
```

Expected output (fails closed):

```
CMS Verification failure
```

`distro/deploy/verify-binary.sh` treats any non-zero exit from that check as fatal — its own final `echo`
+ `exit 1` — so the equivalent on-device failure is the service refusing to start, logged as
`verify-binary: SIGNATURE VERIFICATION FAILED for <bin> — refusing to start`.

## 6. Verifying a downloaded release end-to-end (recipient runbook)

For someone downloading a `sentient-rs` release and wanting full integrity + provenance assurance
before deploying it:

```sh
VERSION=v0.2.0
REPO=High-Velocity-Decisions/sentient-rs
WORKDIR=$(mktemp -d) && cd "$WORKDIR"

# 1. Download everything.
gh release download "$VERSION" --repo "$REPO"

# 2. Checksums: does every file match SHA256SUMS?
sha256sum -c SHA256SUMS

# 3. Provenance: was SHA256SUMS itself produced by this repo's release CI, from this tag?
cosign verify-blob \
  --bundle SHA256SUMS.cosign.bundle \
  --certificate-identity-regexp "^https://github\.com/High-Velocity-Decisions/sentient-rs/\.github/workflows/production\.yml@refs/tags/v.*$" \
  --certificate-oidc-issuer "https://token.actions.githubusercontent.com" \
  SHA256SUMS

# 4. Per-artifact key-based signature — the same check the device performs before it will run
#    the binary. Repeat for each target you're deploying (and each .wic image).
openssl cms -verify -binary -content "bin-rpi-linux-arm64" \
  -in "bin-rpi-linux-arm64.p7s" -inform DER -CAfile ca.pem
```

All three must pass before trusting a release artifact. Step 2 catches accidental corruption or a
mismatched download; step 3 catches a `SHA256SUMS` that was not produced by *this repository's*
release pipeline (wrong identity/issuer = fails closed, §3.2); step 4 is the same gate the device
itself enforces at every service start (§5.2), so verifying it manually here just front-runs a
failure you'd otherwise only discover on the box.

**Current caveat for v0.2.0 specifically:** steps 3 and 4 cannot be completed yet —
`SHA256SUMS.cosign.bundle` and every `.p7s` (plus `signing-cert.pem` / `ca.pem`) are missing from
that release (issue [#105](https://github.com/High-Velocity-Decisions/sentient-rs/issues/105), §1).
Step 2 (`sha256sum -c`) still works today since `SHA256SUMS` itself is present and correctly attached.

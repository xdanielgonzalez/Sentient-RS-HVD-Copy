<p class="eyebrow">Security · Posture</p>

# Security Posture & Recommendations — Sentient METEOR Comms Module

**Scope:** HVD → Sentient METEOR contract, Scope-E consult deliverable (≤10h).
**System under review:** Rust telemetry adapter — Seplos BMS (RS-485) → TMS (MIL-STD-3071) → MQTT — running on a Raspberry Pi comms module, with an Embassy/RP2350 MCU forward path on the roadmap.
**Nature of this document:** a recommendation list + integration path at consult depth. It is **not** a full implementation. Every item is tagged one of:

- **[IMPLEMENTED]** — present in the current code/deploy tree, verified below with `file:line` evidence.
- **[RECOMMENDED - NEXT]** — the next hardening step; buildable on the current Pi without new hardware.
- **[RECOMMENDED - WITH TPM]** — requires the SPI TPM 2.0 add-on recommended in §3.

A short **honesty note** runs throughout: we distinguish *signed/verified boot* (achievable on RPi) from *measured boot / remote attestation* (**not** achievable on RPi — see §2). We do not oversell.

---

## 1. Current code posture — the verified "clean" list

Each claim below was read out of the repository. Evidence is cited as `path:line`.

### 1.1 Embedded broker is loopback-bound by default — [IMPLEMENTED]

The in-process rumqttd MQTT broker binds `127.0.0.1:1883` by default:

- `lib-mqtt/src/broker.rs:33-37` — `impl Default for BrokerConfig` sets `host: "127.0.0.1"`, `port: 1883`.
- `lib-mqtt/src/broker.rs:25-27` — the field doc states the loopback intent explicitly: the mosquitto relay subscribes locally, so the broker "must not be reachable off-host unless a deployment deliberately widens this via config."
- `bin-rpi/src/config.rs:99-103` — the TOML-facing `BrokerToml::default` also defaults to `127.0.0.1:1883`, and `config.rs:233-248` has a test proving only the *default* is loopback (an explicit `host = "0.0.0.0"` in config overrides it).

### 1.2 Residual: the embedded broker is unauthenticated plaintext — accepted, conditionally

The rumqttd broker runs with **no auth and no TLS**:

- `lib-mqtt/src/broker.rs:71` — `auth: None` (also `external_auth: None`).
- `lib-mqtt/src/broker.rs:83` — `tls: None` on the single v4 listener.

This is **acceptable *only* because it is loopback-bound and the local mosquitto bridge is the sole reader** (see §1.3). The trust boundary is the host itself: on-host processes can read the TMS stream unauthenticated.

> **Hard constraint (do not regress):** widening `broker.host` beyond `127.0.0.1` (e.g. to `0.0.0.0` for an off-host subscriber) **requires first adding authentication and TLS to the rumqttd listener.** As shipped, a wide bind would expose an unauthenticated plaintext telemetry feed on the network. The config permits the wide bind (`config.rs:233-248`); the code does *not* gate it — so this is an operational rule, and a candidate for a future startup assertion (`host != loopback ⇒ require tls+auth`).

### 1.3 Cloud bridge TLS shape lives in mosquitto, rendered by `bin-rpi` — [IMPLEMENTED]

Data leaves the device via a mosquitto relay, not the Rust binary's own network path. mosquitto
2.x cannot expand `${...}` env vars in its own config, so the shipped `distro/deploy/mosquitto.conf` no
longer contains a static `connection cloud-relay` block with placeholders — it has only an
`include_dir` (`distro/deploy/mosquitto.conf:50`) pointing at a directory that `bin-rpi
--emit-mosquitto-bridge` renders into before mosquitto starts:

- `bin-rpi/src/mosquitto_bridge.rs` — `render_bridge()` builds the real `connection
  cloud-relay` block (host/port/username/`bridge_cafile`/topic from the typed `[cloud]` config,
  password from the `CLOUD_MQTT_PASS` env var) and always emits `bridge_insecure false` (server
  certificate + hostname validation enforced against the configured CA).
- `distro/deploy/mosquitto-override.conf` — the systemd drop-in that runs the render step as
  `ExecStartPre` and sources the secret via `EnvironmentFile=-/etc/sentient/cloud.env`.
- **Fail-closed:** `render_bridge()` returns `Err` (never a bridge with empty creds) if `[cloud]`
  is present but `host`/`username`/`ca_file`/`CLOUD_MQTT_PASS` is missing or empty — `ExecStartPre`
  failing means mosquitto does not start. Covered by the `fails_closed_when_*` tests in
  `bin-rpi/src/mosquitto_bridge.rs`.
- `write_bridge_file()` / `set_secret_permissions()` write the rendered file `0600`
  (`bin-rpi/src/mosquitto_bridge.rs`).
- `distro/deploy/mosquitto.conf` — the anonymous local listener is bound to `127.0.0.1` (loopback debug
  convenience); the comment says to remove it in production if not needed.

### 1.4 No secrets in source; config is env-injected, password is env-only — [IMPLEMENTED]

- `bin-rpi/src/config.rs` — `Config::load` uses figment: an optional TOML file overlaid with
  `SENTIENT_`-prefixed env vars (`Env::prefixed("SENTIENT_").split("__")`).
- `bin-rpi/src/config.rs:4-6` — module doc: "Nothing (endpoints, credentials, ports) is
  hardcoded."
- The `Config`/`CloudConfig` structs (`config.rs:14`, `config.rs:34`) carry **no password field at
  all** — `CloudConfig` is host/port/username/ca_file/remote_clientid/topic/qos only. The password
  exists solely as the `CLOUD_MQTT_PASS` environment variable
  (`bin-rpi/src/mosquitto_bridge.rs`'s `read_password_secret()`), read once at render time and
  never stored on any struct, so it cannot leak through `Debug`/logging —
  `config.rs`'s `debug_output_never_contains_the_password` test proves this with an env-var
  canary. The Rust binary's own daemon path (the poll loop) never touches cloud creds at all; only
  the `--emit-mosquitto-bridge` render path reads the secret, and only to write it into the
  mosquitto fragment file.
- **Rotation note — a committed TLS bring-up private key was removed.** `lib-link/certs/
  bringup_{cert,key}.der` was a fixed, self-signed P-256 cert/key pair checked into git and
  `include_bytes!`'d into `lib_link::tls` — used only by the test tier (`bin-rpi` does not depend
  on `lib-link`; the MCU build compiles `lib-link` without the `tls` feature), but a private key
  living in git history is a stock finding in any audit regardless of who consumes it. That pair
  is now removed from the tree and **must be treated as revoked** — it was never used for
  anything beyond local bring-up/test handshakes. `lib_link::tls::TlsStage`/
  `lib_link::tls_identity::SoftwareTlsIdentity` no longer embed or generate any cert/key material
  at all; every identity is a plain constructor parameter, and this workspace's tests source theirs
  from `test_kit::tls::generate_bringup_identity` (`rcgen`, `ring` backend) — a fresh, self-signed
  identity generated **per process**, never written to disk, never checked in.

### 1.5 `unsafe` is confined to the MCU firmware binary — [IMPLEMENTED]

- `#![forbid(unsafe_code)]` on the four core crates: `lib-parse/src/lib.rs:12`, `lib-mapping/src/lib.rs:12`, `lib-io/src/lib.rs:11`, `lib-mqtt/src/lib.rs:10`. (`forbid` cannot be locally overridden — a hard floor.)
- `impl-app` — `#![forbid(unsafe_code)]` at crate level (the shared loop + host platform are entirely unsafe-free).
- The MCU platform adapter that replaced the retired Zephyr one — `bin-mcu-platform`
  (`crates/bin/mcu-platform/`, the Embassy `spec_app::Platform` impl, ADR
  [0010](../decisions/0010-mcu-runtime-embassy.md)) — contains **no `unsafe` at all**: the C shim
  and its FFI bindings, which used to be the sole `unsafe` island, went away with the Zephyr tree.
- `unsafe` now lives only in the excluded MCU **binary** workspace `crates/bin/mcu-rp2350` — the SAU /
  MPU configuration, the NS-world jump, the CMSE veneer entry points and a few `static`-backed
  buffer pools, each carrying a `// SAFETY:` invariant comment. Its frozen ABI crate
  (`crates/bin/mcu-rp2350/abi/src/lib.rs:39`) is itself `#![forbid(unsafe_code)]`.
  **Honest caveat:** that code is hardware-privilege configuration rather than FFI marshalling, so
  it is a *different* — and, on the security boundary, more consequential — class of `unsafe` than
  the C-shim bindings this section used to describe. It is gated by build/link only; see the
  no-RP2350-emulation gap in §5.

### 1.6 No `unwrap`/`expect`/`panic` in logic — [IMPLEMENTED]

- Root `Cargo.toml:52-56` — `[workspace.lints.clippy]` sets `unwrap_used = "deny"`, `expect_used = "deny"`, `panic = "deny"`.
- Gate: `cargo clippy --workspace --all-targets -- -D warnings` (`Cargo.toml:51`). Test code opts out with scoped `#[allow]` (e.g. `config.rs:151-152`), so the deny applies to production logic only.
- `Cargo.toml:57-62` — release profile is `panic = "abort"`, `opt-level = "z"`, `lto = true`, `strip = true`.

### 1.7 Bounded (heapless) memory on the core path — [IMPLEMENTED]

- `Cargo.toml:26` — `heapless = "0.8"` is a workspace dep; `Cargo.toml:29` — `serde-json-core` (no_std, no-alloc) for the TMS codec.
- `impl-app/src/lib.rs:8` — `#![cfg_attr(not(feature = "host"), no_std)]`: the core compiles `no_std` on the MCU path.
- The MCU firmware is `#![no_std]` with no global allocator at all (`crates/bin/mcu-rp2350/ns`, Embassy): the shared parse/map/codec path does not touch a heap on either target.

**Summary of §1:** the surface the customer's data crosses is tight — loopback-only broker, TLS + validated CA to the cloud, no in-repo secrets, unsafe fenced to one audited FFI module, no panicking control flow, bounded memory. The one residual (unauthenticated plaintext broker) is contained by the loopback bind and governed by the operational rule in §1.2.

---

## 2. Verifiably-trusted binary — RPi boot integrity (stated honestly)

This is the crux of the engagement, and the place it is easiest to oversell. The Raspberry Pi is **not** a measured-boot platform. We tier the controls by what they actually prove.

### Tier 1 — binary integrity at service start — [IMPLEMENTED]

Available today, no irreversible hardware step:

- **Detached binary signature check.** `distro/deploy/verify-binary.sh` verifies the release binary against a detached X.509 CMS signature before it may run: it runs `openssl cms -verify -binary -content "$BIN" -in "$SIG" -inform DER -CAfile "$CAFILE"` (the `.p7s` detached signature against our self-signed CA); any failure exits non-zero. Signing is done offline on the build host / in CI with an offline signer key (`openssl cms -sign …`). Full key/cert-creation/CI-secret/verify walkthrough (plus the separate cosign release-provenance mechanism): [`signing-and-verification.md`](./signing-and-verification.md).
- **systemd gate.** `distro/deploy/bin-rpi.service:29` — `ExecStartPre=/usr/local/bin/verify-binary.sh /usr/local/bin/bin-rpi`. A non-zero `ExecStartPre` blocks `ExecStart`, so a tampered or swapped binary does not start.
- **Hardened unit sandbox.** `bin-rpi.service:42-63` — `NoNewPrivileges`, `ProtectSystem=strict`, `ProtectHome`, `PrivateTmp`, `ProtectKernel{Tunables,Modules,Logs}`, `MemoryDenyWriteExecute`, `RestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX`, `CapabilityBoundingSet=` (drop all), `SystemCallFilter=@system-service`, `DevicePolicy=closed` + `DeviceAllow=/dev/ttyAMA0 rw` (only the BMS serial line).
- **Liveness watchdog.** `bin-rpi.service:21-24` — `Type=notify`, `WatchdogSec=30`; the app pets the watchdog only while healthy, so a wedged loop is restarted.

**What Tier 1 proves:** the *running binary* matches the signed artifact **at the moment systemd launched it**, and the process runs least-privileged. **What it does not prove:** that the *kernel, initramfs, bootloader, or the `verify-binary.sh` script itself* are unmodified — those are trusted implicitly. Tier 1 is a strong integrity control for the application layer; it is not root-of-trust boot verification. That is Tier 2.

### Tier 2 — RPi signed (verified) boot + read-only rootfs — [RECOMMENDED - NEXT]

To extend the trust root below the application, on RPi4/RPi5:

- **EEPROM signed boot.** Enable `SIGNED_BOOT=1` via `rpi-eeprom-config`, sign the boot image with a customer-held **RSA-2048** key, and burn the **public-key hash into OTP**. The bootloader then signature-verifies `boot.img` before executing it. This is **verified boot** — a modified boot image will not run.
  > **The OTP burn is IRREVERSIBLE.** Once the public-key hash is fused, that device is permanently locked to that key; a lost private key bricks the field-provisioning path for that unit. Pilot on sacrificial hardware, script the provisioning, and hold the signing key in the same offline custody as the X.509 CMS signer key before burning any production unit.
- **dm-verity read-only rootfs.** Build the rootfs as a dm-verity hash-tree volume; place the **root hash in the kernel command line**, which is itself covered by the signed boot image. Any block-level tampering of the rootfs is detected on read. Mount mutable state (config, logs, mosquitto persistence) via an **overlayfs** upper layer on a separate writable partition.

**What Tier 2 proves:** an unbroken *verify-then-execute* chain from immutable OTP → boot image → kernel + cmdline → (via the cmdline root hash) the entire rootfs. Combined with Tier 1, the application binary is then verified by a rootfs that is itself verified from a hardware-fused key.

### Signing-key custody — signing binaries without a key in CI — [RECOMMENDED]

Both Tier 1 (the `verify-binary.sh` detached signature) and Tier 2 (the OTP-fused boot key) depend on a **private signing key**. The security question is *where that key lives when the release pipeline signs it*. The weakest posture — a private key stored as a CI / GitHub Actions secret — is explicitly avoided: a secret in CI is exfiltrable by any workflow with access to it and is not hardware-bound.

**Recommended: HashiCorp Vault as the signer (the key never leaves Vault).** `cosign` — already used for release provenance and baked into the build image — supports Vault's Transit secrets engine as a KMS backend:

```
# key ref is hashivault://<transit-key-name>; the transit mount is set via the
# TRANSIT_SECRET_ENGINE_PATH env var (default "transit"), not in the URI path
cosign sign-blob --key hashivault://sentient-signing \
  --tlog-upload=false --bundle SHA256SUMS.cosign.bundle SHA256SUMS
```

- **Key custody.** The signing key is generated and held inside Vault; the pipeline calls Vault's `transit/sign` API (ACL: `update` on `transit/sign/sentient-signing`) and receives only the signature. The private key is never materialised in CI, in a GitHub secret, or on the build host. Transit signs with ECDSA (P-256/384/521) or Ed25519 — a raw signature via cosign's `--key`, not the `openssl cms` detached-signature flow the on-device gate currently verifies (see the on-device change below).
- **Keyless CI → Vault auth.** GitHub Actions' OIDC token (`id-token: write`) authenticates to a Vault `auth/jwt`/`auth/oidc` role (`bound_issuer`/`bound_claims` scoping repo+ref), minting a short-lived, auto-revoked Vault token per run — so there is no long-lived Vault credential stored in CI either.
- **Offline device verification — with a version caveat.** Because we sign with an explicit `--key` (not keyless/Fulcio), verification needs only the exported public key and no Sigstore infrastructure: `cosign verify-blob --key sentient-signing.pub --bundle … --insecure-ignore-tlog=true …`. **Two operational must-dos:** (1) the verify side must *also* skip the transparency-log check (`--insecure-ignore-tlog=true`) or cosign fails closed looking for a Rekor inclusion proof; (2) **pin and test a specific cosign version** — `--tlog-upload=false` was **silently not honoured in cosign v3.0.2** (the entry still uploaded to public Rekor; [sigstore/cosign#4503](https://github.com/sigstore/cosign/issues/4503)), which would break the air-gap guarantee. Validate on the pinned version that no Rekor upload occurs before relying on the offline claim.
- **Assurance tiering.** Vault **OSS** Transit protects the key in **software** inside Vault (a large improvement over a CI secret, but not hardware custody). Vault **Enterprise** with **Managed Keys** (Transit sign/verify delegated to an external HSM over PKCS#11) makes the Transit key **hardware-backed** — key material lives in the HSM, not Vault; seal-wrap adds an HSM-encrypted layer over Vault's stored data. Both are Enterprise-only; OSS Transit is software-key-material.

**One consequential change:** Vault Transit produces a raw ECDSA/Ed25519 signature via cosign's `--key`, not the X.509 CMS detached signature the on-device gate currently verifies, so adopting it moves the gate from `openssl cms -verify` to `cosign verify-blob --key …` in `verify-binary.sh`. `cosign` is a heavier on-device dependency than the already-present `openssl`; that is the trade for a Vault-custodied key and a single signing tool spanning release provenance and binary signing.

**Alternative — an air-gapped YubiKey 5 FIPS signer (§3).** Where Vault keeps the key in a networked KMS the CI pipeline calls *automatically*, a **YubiKey 5 FIPS** (~$88–$98 for the USB-A/USB-C SKUs; the 5Ci is ~$115. FIPS **140-3 Overall Level 2 / Physical Security Level 3**, Cert #5291, on firmware **≥ 5.7.4** — older 5.4.x stock is FIPS 140-2 and now on NIST's sunset list, so specify firmware when procuring) holds the release-signing private key on a hardware token that an engineer physically **touches** to sign each release (PIV/OpenPGP slot with `touch-policy=always`). This is the strongest custody — the private key never exists on any build machine and the token is never network-reachable. A YubiKey PIV slot can hold the X.509 CMS signer key and issue the same `openssl cms` detached signatures the *existing* gate verifies (so `verify-binary.sh` is unchanged); alternatively a PGP/cosign-`--key` slot signs, which moves the gate as in the Vault case above. Its cost is operational: signing requires human presence, so it does not fit a fully-automated CI release. The choice is therefore **manual-but-strongest (YubiKey, air-gapped human-touch) vs automated-and-convenient (Vault, pipeline-called KMS)** — both keep the key off CI and off disk; pick per whether releases are cut by hand or by the pipeline. (The YubiKey is a *build-pipeline* device only; §3 explains why it is **not** suitable as the device-side TPM.)

**How this relates to the TPM (§3) — distinct layers, do not conflate:**
- **Vault = the *build-time* signer** — it custodies the key that signs the *release binary*, off the device.
- **The TPM = the *device-time* root of trust** — it protects the *runtime* device identity key (bridge TLS / MQTT-token, §3) and can hold the *verification* public key, on the device.

They are complementary: Vault signs what the device later verifies; the TPM protects the device's own keys and anchors the verify step. Neither provides RPi boot *attestation* — that remains impossible on this hardware (see immediately below).

### What does NOT exist on RPi — measured boot / remote attestation

**The Raspberry Pi has no CRTM (Core Root of Trust for Measurement) that measures each boot stage into a TPM PCR from reset.** The GPU-based boot ROM does not extend measurements into a TPM.

- **Consequence:** a bolt-on SPI TPM (see §3) **cannot attest the boot chain on RPi.** There is no reset-anchored measurement chain for it to record, so its PCRs cannot be used to prove "this device booted known-good firmware" to a remote verifier.
- **Do not recommend, and we explicitly do not:** TPM-based *boot attestation* or *remote attestation* on RPi. Any proposal that claims the Pi + a TPM can remotely attest its boot state is incorrect for this hardware.
- **Verified boot (Tier 2) is the correct RPi answer** to "did the device boot trusted software" — it *enforces* it locally at boot, rather than *measuring and reporting* it. The TPM's value here is **key protection**, not attestation (§3).

> If remote boot attestation is a hard contractual requirement, it needs a platform with a measured-boot root of trust (e.g. an SoC with a hardware secure-boot + measured-boot pipeline, or a discrete TPM wired into a firmware that implements SRTM). That is a hardware-selection decision, out of scope for the current RPi module.

---

## 3. Hardware TPM recommendation — SPI TPM 2.0 for RPi — [RECOMMENDED - WITH TPM]

A discrete SPI TPM 2.0 is worthwhile on this platform **for key protection**, not for attestation (§2). The TPM is the *device-time* root of trust; it is distinct from the *build-time* release signer (**HashiCorp Vault**, §2 "Signing-key custody") — Vault custodies the key that signs the binary off the device, the TPM protects the device's own runtime keys and can hold the verification public key on the device.

**Product & pricing recommendation (TL;DR).** Three distinct layers, three distinct devices — do not conflate them:

| Layer | Recommended device | Est. price | Purpose |
|---|---|---|---|
| **Field device (RPi)** | Infineon **SLB9672** SPI TPM HAT (LetsTrust ~£26/$26–$33, or ANAVI TPM 2.0); spec an **extended-temp variant** for sub-0 °C/hot enclosures | ~$26–$37/unit | LUKS data-key seal, non-exportable bridge-TLS client key, device identity key |
| **Build pipeline (signer)** | **YubiKey 5 FIPS** (air-gapped human-touch) *or* HashiCorp Vault (automated KMS, §2) | $88–$98 (YubiKey, fw ≥ 5.7.4) | Protect the release-signing private key — never on a build host or in CI |
| **Cloud CA (future)** | **YubiHSM 2 FIPS** (FIPS 140-3 L3, v2.4) | $950 | Hardware-rooted CA if Sentient issues client certs to field units |

**Why not an HSM for the field device.** Enterprise HSMs (Thales Luna, Entrust nShield, AWS CloudHSM — $1,000–$50,000 appliances or ~$1.60/hr cloud) are built for centralized, multi-user PKI roots. The device threat model here is **one unit → one hardware root of trust**, which is exactly what a discrete TPM is for. An HSM on the field device is architectural overkill; the right place for hardware-rooted key custody at the fleet level is a **cloud-side CA** (YubiHSM 2, above), not the RPi.

Recommended device-side parts:

| Part | Board examples | Device-tree overlay | Notes |
|---|---|---|---|
| **Infineon SLB9672** (prefer) | LetsTrust TPM (current product is the 9672, ~£26/€25 ≈ $26–$33), ANAVI TPM 2.0 (open-source SLB9672, CrowdSupply, shipping ~mid-2026) | `dtoverlay=tpm-slb9670` | Current generation; **field firmware-updatable**. Kernel driver `tpm_tis_spi`. Prefer for new designs wanting long support life. |
| **Infineon SLB9670** (fallback) | GeeekPi TPM9670 (~$30, US-stocked), generic SLB9670 HATs | `dtoverlay=tpm-slb9670` | Mature, widely deployed, best documentation/community support. Acceptable only if a genuine SLB9672 board is unavailable. |

> **Overlay note (corrected):** there is **no separate `tpm-slb9672` overlay** in the Raspberry Pi kernel tree — only `tpm-slb9670-overlay.dts` exists, and Infineon documents it as driver-compatible with the SLB9672. Use `dtoverlay=tpm-slb9670` for **both** chips; verify the silicon on the board, not the overlay name. (The discontinued Infineon IRIDIUM SLB9672 eval kit is eval-only and out of stock at US distributors — not a deployment part.)

**Compatibility caveats:**
- **Pi 5 SPI header:** the 40-pin header remains, but Pi 5's SPI is driven by the RP1 southbridge — confirm the overlay resolves the correct SPI controller on Pi 5 and use a current kernel. Overlays and `tpm_tis_spi` binding have historically been Pi-4-tested first; validate on the exact Pi 5 + kernel + `rpi-eeprom` combination you ship.
- **Kernel/overlay version:** `tpm-slb9670` / `tpm-slb9672` overlays and the `tpm_tis_spi` driver need a reasonably current kernel. Pin the kernel/overlay versions alongside the Yocto layer/rootfs pins and re-test after any bump.
- Expose the char device (`/dev/tpm0`, `/dev/tpmrm0`) to the service only if a TPM-backed feature actually uses it — keep `DevicePolicy=closed` and add a narrow `DeviceAllow` rather than widening device access.

**Why SLB9672 for a new design.** The SLB9672 is strictly a superset of the older SLB9670: RSA to **4096-bit** (vs 2048), ECC **P-256 + P-384** (vs P-256 only), AES-128/256 (AES-192 is optional per the datasheet, so don't assume it), **field firmware-update** (the world-first XMSS post-quantum FW-update path), and a wider temperature envelope. **Temperature — state precisely:** the *base* SLB9672 is -20 °C…+85 °C like the SLB9670; the **−40 °C…+85 °C ("enhanced")** and **−40 °C…+105 °C (extended)** ranges are *variant/special-order* options, not the default part — spec the extended-temp variant explicitly if the enclosure sees sub-0 °C or >85 °C, don't assume a stock HAT provides it. Both chips are FIPS 140-2 Level 2 (SLB9672 also holds two 2024 firmware-specific Common Criteria EAL4+/AVA_VAN.4 certs) and use the same `tpm_tis_spi` driver, so there is no integration cost to preferring the current silicon; the SLB9670 is an acceptable fallback only if a genuine SLB9672 HAT is unavailable.

**Userspace integration path:** `tpm2-tools` + the `tss2` stack (tpm2-tss / `libtss2`), with `tpm2-pkcs11` for TLS key storage and `clevis` (+ `clevis-luks`) for disk-key sealing. **From Rust:** the [`tss-esapi`](https://crates.io/crates/tss-esapi) crate (safe bindings over the ESAPI layer, maintained by parallaxsecond) is the primary path; a wolfTPM-based alternative exists as `tpm2-rs` (wraps the wolfTPM C library, so it needs system-installed wolfSSL/wolfTPM — not a pure-crates.io dependency). Neither is on the critical path — the shipping code does not depend on a TPM.

### Sound TPM uses **without** measured boot (frame these as key-protection wins)

1. **Seal the LUKS data-partition key to the TPM — [RECOMMENDED - WITH TPM].** Encrypt the writable data/state partition with LUKS and bind the unlock key to the TPM via `clevis`/`tpm2` (a `tpm2` pin). An SD card or eMMC image **exfiltrated from the device does not decrypt** off-platform, because the key never leaves the TPM. (Note: without measured boot you seal to the TPM's *presence*, optionally with a TPM auth policy, rather than to a PCR set that proves boot state — still defeats the "pull the card and read it elsewhere" threat, which is the realistic field-loss case for a comms module.)
2. **Hold the mosquitto cloud-bridge TLS *client* key in the TPM via `tpm2-pkcs11` — [RECOMMENDED - WITH TPM].** If the cloud bridge is upgraded to mutual TLS (client cert), store the client private key as a non-exportable TPM object (`tpm2-pkcs11` marks keys `CKA_SENSITIVE`/`CKA_NEVER_EXTRACTABLE`) and let mosquitto use it through the PKCS#11 provider. The device credential becomes **non-exfiltrable** — a compromised rootfs cannot copy the key off the box; it can at most use it while resident. **Implementation risk to plan for:** mosquitto's PKCS#11 path differs by OpenSSL major version — OpenSSL 1.1 uses the `-engine pkcs11` interface, OpenSSL 3 uses `-provider tpm2`. A mosquitto built against OpenSSL 3 but configured for the *engine* interface will **silently fail to use the TPM key** — pin and test the engine-vs-provider path for the target image. This complements §4 (today the bridge authenticates with username/password + server-CA; a TPM-held client key is the hardening path if Sentient issues client certs).
3. **Device identity key for future provisioning — [RECOMMENDED - WITH TPM].** A TPM-resident identity keypair gives each unit a stable, hardware-bound identity for enrollment/rotation, without a private key ever touching the filesystem.

All three are **key-protection** wins (secrets that cannot be stolen off a captured device), **not** attestation (proving to a remote party how the device booted). Keep that framing in customer conversations.

### Where each device fits — architecture fit matrix

| Use case | SLB9672 TPM (~$35) | YubiKey 5 FIPS ($88–98) | YubiHSM 2 FIPS ($950) |
|---|---|---|---|
| Device LUKS disk-key seal | Yes — best (clevis/tpm2) | No | No |
| Device TLS client key (non-exportable) | Yes — `tpm2-pkcs11` | No — human presence required | Conditional — temp risk + cost |
| Device identity / enrollment key | Yes | No | No |
| **Build-pipeline binary signing key** | No — wrong form factor | Yes — best (air-gapped) | Yes — overkill |
| Cloud-side CA / device-cert issuance | No | No | Yes — best |
| FIPS 140-3 Level 3 key store | No — Level 2 only | No | Yes |

**YubiHSM 2 FIPS — cloud CA only, never the field device.** At $950 the current **v2.4 is FIPS 140-3 Overall Level 3** (Cert #5302; the earlier v2.2 was FIPS 140-2 Overall Level 2 / Physical-Security Level 3 — cite the 140-3 part for an unambiguous claim), with 256 object slots (~128 KB aggregate) and a tamper-evident audit log — exceptional value versus a rack-mount HSM ($10k–$50k, i.e. ~1/10 to 1/50 the cost) **if** Sentient stands up a device-cert CA. But it is rated **0 °C to 40 °C operating** (−20/85 storage): a tactical field enclosure (cold-weather ops below 0 °C, solar-heated cabinets above 40 °C) will violate that, which alone disqualifies it as a device-side part. An **extended-temp SLB9672 variant** is the field answer; reserve the YubiHSM 2 for the climate-controlled server side (integrates via `yubihsm_pkcs11.so` or the `yubihsm` Rust crate).

### Adversarial caveats — do not ship the TPM path without these

- **A LUKS break-glass recovery key is mandatory.** If the data partition is sealed to the TPM and the TPM (or its state) is lost, the partition is **permanently unrecoverable** without a documented recovery key held in secure offline custody. This is a ship-blocker for use 1, not an optional extra.
- **Validate the Pi 5 SPI binding on the exact image you ship** (RP1 southbridge, see the compatibility caveats above) — the overlays have historically been Pi-4-first.
- **Supply:** SLB9672 HATs are well-stocked via EU distributors (Pi Hut ~£26, reichelt/buyzero ~€25); US retail is thinner (GeeekPi SLB9670 ~$30 is the readily-US-stocked fallback; the Infineon eval kit is discontinued). The **ANAVI TPM 2.0** (open-source KiCad, CrowdSupply, shipping ~mid-2026) covers the gap, and for 100+ units a custom PCB carrying a bare SLB9672 die (**~$5–$8/unit is an order-of-magnitude estimate — confirm with a Digi-Key/Mouser die quote + a JLCPCB/PCBWay PCBA quote**; ANAVI's design as reference) is the production-scale path.
- **Integration effort fits the ≤10h consult cap:** bring-up 1–2h + LUKS seal 2–3h + PKCS#11 key provisioning 2–3h (deferrable until mutual TLS is agreed) + docs 1–2h ≈ **6–10h**.

---

## 4. Data-in-transit & credential hygiene — [IMPLEMENTED]

- **TLS is enforced in the mosquitto bridge** (§1.3): `bridge_cafile` is always set from the `[cloud] ca_file` config value and `render_bridge()` always emits `bridge_insecure false` → server certificate and hostname are validated against the provided CA.
- **The Rust binary's daemon path never handles cloud credentials.** Only the separate
  `--emit-mosquitto-bridge` render path reads `CLOUD_MQTT_PASS` (§1.4), and only to write it,
  once, into the mosquitto fragment file — it is never stored on a struct, logged, or held past
  that write.
- **Credential storage:** `/etc/sentient/cloud.env` (the secret env file) and the rendered
  `/etc/sentient/mosquitto.d/cloud-relay.conf` must both be `0600` and owned by the user mosquitto
  runs as — **never committed to git** (`.gitignore` blocks `cloud.env` and `cloud-relay.conf`
  by name; only `distro/deploy/cloud.env.example`, a placeholder template, is tracked). The CA
  certificate (`ca.crt`) and the binary's detached `.p7s` signature deploy root-owned `0644`.
- **Day-1 Sentient credentials** (cloud endpoint, username, password, CA — and, if mutual TLS, the
  client cert/key) go into the typed `[cloud]` config (non-secret fields) plus
  `/etc/sentient/cloud.env` (the password), not anywhere in git. See
  `docs/deployment.md` §9 for the exact onboarding runbook. Hardening path: promote the bridge to
  mutual TLS and hold the client key in the TPM (§3, use 2).

---

## 5. Embassy/RP2350 MCU forward path — security notes (doc depth)

**This section was rewritten when the MCU left Zephyr.** ADR
[0010](../decisions/0010-mcu-runtime-embassy.md) retired the Zephyr MCU target and replaced it with
**Embassy on the RP2350** (Cortex-M33); `bin/zephyr`, its `prj.conf`/`west.yml`, and the
`bin-zephyr-platform` adapter no longer exist, so the MCUboot-via-sysbuild path and the Zephyr
Kconfig ship-blockers this section used to list are **gone, not merely deferred**. The MCU remains a
**secondary** target that must never block the RPi deliverable. The security boundary design for it
is ADR [0011](../decisions/0011-mcu-security-boundary.md) (TrustZone S/NS split, veneer ABI); what
follows is what is *actually* true of `crates/bin/mcu-rp2350` today.

### Firmware signing — an open gap, not a recommendation — [GAP]

The Zephyr path had a concrete answer here (MCUboot verifying an imgtool-signed image before boot,
with confirm/revert for safe unattended updates). **There is no in-tree Embassy equivalent, and this
document is not going to invent one.** What exists:

- `just mcu-rp2350-uf2` prints the picotool steps to combine the `s` + `ns` ELFs and sign the
  IMAGE_DEF block with a **demo secp256k1 key**. That is a load-time-integrity / IP-protection
  signature only — **not** a verified boot chain, not fused to OTP, and the recipe says so.
- The RP2350 silicon does provide an OTP-fused secure-boot capability, but this build does **not**
  use it, and no key-provisioning or anti-rollback story has been written.
- A real bootloader (and the wolfCrypt-FIPS-via-FFI crypto module behind it) is **Gate-0-blocked**
  (issue #351) pending the written FIPS-scope / CNSA / attestation / operating-environment answers.
  ADR 0010 is explicit that it ships the *runtime*, not a secure-boot chain.

So the §2 asymmetry that used to favour the MCU — "the Pi cannot do reset-anchored verified boot,
the MCU can" — is currently a **claim of the security architecture, not a demonstrated control**.
Anything stronger than "the chip is capable of it" would be overselling.

### Ship-blockers on the RP2350 path (all security-relevant)

1. **No hardware entropy.** The secure world seeds its session-key RNG from the Cortex-M `DWT` cycle
   counter via `lib_security::backends::software::InsecureTestRng`, a splitmix64 generator
   (`crates/bin/mcu-rp2350/s/src/service.rs`, and the crate's own README §"Entropy"). This is
   **explicitly not a TRNG** and is documented as such at the source. The RP2350 datasheet describes
   a ring-oscillator TRNG peripheral; it is not wired up (TODO #355). No real key material may
   depend on this build until it is.
2. **DMA bypasses the SAU.** The SAU gates CPU-core accesses only. Stopping a compromised NS-side
   DMA descriptor from reaching secure memory needs the RP2350's bus-level access-control
   peripheral, which is **not** configured here (`crates/bin/mcu-rp2350/s/src/sau.rs`, TODO #355). Until
   it is, the S/NS split is a CPU-access boundary, not a full memory-isolation boundary.
3. **The S image is unproven on real silicon.** `rp2350-s` compiles and links (SAU config, veneer
   service, `bxns` handoff), but there is **no RP2350 emulation** — neither Renode nor QEMU models
   the chip (ADR 0010) — so nothing in CI demonstrates the image is boot-ROM-loadable. On-target
   confirmation is HIL smoke on the bench, which is not a merge gate.
4. **No MQTT/TLS egress on the MCU yet.** `ns` brings up the W5500 + `embassy-net` + DHCPv4, but no
   TCP/MQTT socket rides that stack yet, and the MAC address is a fixed non-unique
   locally-administered constant (`crates/bin/mcu-rp2350/README.md`, TODO #359). The transport-security
   questions the Zephyr entry above raised for MQTT-over-TLS therefore have no answer on this path
   either — there is simply nothing publishing yet.

The MCU path does still inherit the core's `no_std`/heapless discipline and the
`forbid(unsafe_code)` floor on the shared crates (§1.5–1.7). What changed is *where* the `unsafe`
sits: no longer a C-shim FFI island, but the SAU/MPU/veneer/NS-jump code in the binary itself
(§1.5).

---

## 6. Recommended tiers — summary

| Tier | Control | Status | What it buys | Cost / caveat |
|---|---|---|---|---|
| **1 — Now** | X.509 CMS signature check (`ExecStartPre`, `openssl cms -verify` against the CA) + hardened systemd unit (sandbox + watchdog) | **[IMPLEMENTED]** | Running binary matches the signed artifact; least-privilege process; auto-restart on wedge | Trusts kernel/bootloader implicitly; not a hardware root of trust |
| **2 — Next** | RPi EEPROM signed boot (`SIGNED_BOOT=1`, RSA-2048, OTP key hash) + dm-verity read-only rootfs + overlayfs | **[RECOMMENDED - NEXT]** | Verified-boot chain from fused OTP → boot image → kernel → rootfs | **OTP burn is IRREVERSIBLE**; pilot on sacrificial units; guard the signing key |
| **3 — With TPM** | SPI TPM 2.0 (SLB9672 HAT, ~$35/unit): LUKS data-key seal (clevis/tpm2) + bridge TLS client key in `tpm2-pkcs11` (+ device identity key) | **[RECOMMENDED - WITH TPM]** | Exfiltrated SD card won't decrypt; device credential non-exfiltrable; hardware-bound identity | **Key protection only — NOT boot attestation** (RPi has no measured-boot CRTM, §2). **Mandatory:** offline LUKS break-glass key (§3) |
| **(MCU path)** | Signed/verified boot on the RP2350 | **[GAP]** — no in-tree mechanism | *Would* buy reset-anchored verified boot + safe field updates on the MCU | The Zephyr MCUboot answer went away with the Zephyr tree (ADR 0010); today there is only a picotool **demo** IMAGE_DEF signature, not a fused root of trust. A real bootloader is Gate-0-blocked (#351). Also blocked on: no hardware TRNG, DMA not gated against the SAU, no RP2350 emulation to gate on (§5) |

**Bottom line.** The application layer is already tight and honestly-scoped (§1). The near-term boot-integrity story on the Pi is *verified boot* (Tier 2), not attestation — and the TPM's honest role on this hardware is *key protection* (Tier 3), not proving how the device booted. Anyone promising RPi remote boot attestation with a bolt-on TPM is wrong for this platform; the correct answers are signed boot + read-only rootfs, plus a TPM for non-exfiltrable keys.

---

## 7. Cryptographic profile — per-purpose key custody & algorithm approval (`lib-security`)

Two more posture facts belong in this report for the same reason §1–§6 spell everything out explicitly: a single device-wide custody claim goes stale the moment different key purposes are held differently, and an algorithm's `NotApproved` status needs to say *why*, or an engineer "fixes" it incorrectly.

### 7.1 Per-purpose custody — the mixed-custody honesty fix

`lib-security`'s `SecurityProfile` tracks custody **per key purpose**, not as one device-wide tier: `identity_custody`, `session_custody`, and `validation` (FIPS-140-3 vs standard) are three independent fields. This replaces an earlier single `key_custody` field that made one claim for the whole device — a claim that becomes a lie the moment identity keys move to a hardware secure element while session keys stay in software.

`KeyCustody` gains a third, strongest variant. Strength ordering (weakest → strongest):

| Custody | Meaning | Strength |
|---|---|---|
| `SoftwareMemory` | Keys live in normal-world process memory (zeroized on drop) | weakest |
| `SecureWorld` | Keys live behind an ARM TrustZone / TPM boundary, referenced by opaque handle (`trustzone` feature) | middle |
| `SecureElement` | Keys are born and held inside a dedicated hardware secure element, never resident in host memory even transiently (new `secure-element` cargo feature — governs `identity_custody`) | strongest |

`tier_slug()` reports `validation` + the **weakest** custody present across all purposes, never the strongest — so a build cannot summarize itself as more assured than its weakest key actually is. The four historical, byte-stable slugs are unchanged and remain the only strings the summary ever emits: `standard+software`, `standard+trustzone`, `fips+software`, `fips+trustzone`. `SecureElement` never surfaces in a slug by itself: `secure-element` upgrades identity custody only, and `session_custody` (software or trustzone) is always present alongside it, so the weakest-of-both-purposes rule always caps the reported tier at `software` or `trustzone` even when identity keys sit in the secure element. `custody_warning()` is updated to name the specific *weak purpose* (e.g. `session key custody = SoftwareMemory …`) rather than a single undifferentiated device-wide warning, so an operator reading the boot log knows exactly which key class is the exposed one — not merely that "some key somewhere" is software-held.

**Why this matters:** before this change, a device with identity keys in a secure element but session keys still in software memory would have reported one blended (and effectively overclaiming) custody tier. The per-purpose model makes the summary honest in the same way §1.2 and §2 refuse to oversell the RPi's boot posture — it reports the *weakest* link, and names it.

### 7.2 Ed25519 stays `NotApproved` — a module-certificate gate, not a standards gate

`Provider::check_approved` (§3 of [`architecture.md`](../architecture.md), backed by `Algorithm::is_fips_approved` in `lib-security`) continues to classify Ed25519 as `NotApproved`. That is **not** because Ed25519 is disallowed by the signature standard:

- **FIPS 186-5** (Feb 2023) specifies Ed25519 outright — the algorithm itself is standards-approved.
- The gate is a **module-certificate** fact, not a standards fact: our validated crypto module, wolfCrypt, does **not** list Ed25519 on either of its FIPS 140-3 certificates (**#4718**, **#5041**) for our operating environment. An algorithm the standard allows is still `NotApproved` here until the *certified module* we actually run says it covers it.
- **CNSA 2.0** independently excludes Ed25519 for National Security Systems use, mandating **ML-DSA-87** in its place — a second, orthogonal reason not to flip this gate casually even once a covering module certificate exists.

**Do not flip `is_fips_approved()` to `true` for Ed25519 without a validated module certificate that explicitly lists it for our operating environment** — doing so under an NSS designation would itself be a CNSA violation, not merely a paperwork gap. **Gate-0 tracking issue #351** owns the recheck: watch for a wolfCrypt certificate revision (or a future cert) that lists Ed25519, and re-evaluate against CNSA 2.0 scope before any change.

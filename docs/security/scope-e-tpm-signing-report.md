# Scope E: Verifiably-Trusted Binary and Hardware Key Protection

**Consolidated recommendation. Customer deliverable.**

**Contract:** HVD to Sentient METEOR, Scope-E consult (≤10 h).
**System:** Rust telemetry adapter. Data path: Seplos BMS (RS-485) to TMS (MIL-STD-3071) to MQTT, running on a Raspberry Pi comms module. An Embassy/RP2350 MCU forward path is on the roadmap.
**Purpose:** a recommendation and integration path at consult depth for two things: (a) signing release binaries so a device can verify them, and (b) hardware-protecting the device's own keys. This is not an implementation. §6 fact-checks every load-bearing claim against vendor and primary sources and lists the open items that need a human spot-check.

This report consolidates SPIKE #63 (TPM options) and SPIKE #64 (verifiably-trusted binary) and is the single source of truth for both. It extracts and expands [`README.md`](./README.md) §2–§3. The broader security posture (code review, data-in-transit, the MCU path) stays in that document.

---

## 0. Executive Summary

This report covers three layers. Each uses a different device. One rule governs all three: the private signing key never lives on a build host or in a CI secret, and the device's own keys never leave hardware.

| Layer | Recommended device / approach | Est. price | What it protects |
|---|---|---|---|
| **Build pipeline (signer)** | **HashiCorp Vault Transit + cosign** (automated CI), or **YubiKey 5 FIPS** (air-gapped, human-touch) | $0 sw / $88–$98 YubiKey | The release-signing private key, off the build host and off CI |
| **Field device (RPi)** | Infineon **SLB9672** SPI TPM 2.0 HAT (extended-temp variant for harsh enclosures) | ~$26–$37/unit | Device runtime keys: LUKS disk key, bridge-TLS client key, device identity |
| **Cloud CA (future)** | **YubiHSM 2 FIPS** (climate-controlled server side only) | $950 | A hardware-rooted CA when Sentient issues client certs to the fleet |

### Trust Model at a Glance

```
   BUILD TIME  (off-device)                     DEVICE TIME  (on-device, Raspberry Pi)
   ------------------------------                ---------------------------------------
   Signing-key custody                           Runtime-key protection
     - Vault Transit  (automated CI)               SLB9672 SPI TPM 2.0
     - YubiKey 5 FIPS (air-gapped, manual)           - LUKS data-key seal
            |                                         - TLS client key (tpm2-pkcs11)
            |  signs                                  - device identity key
            v
     release binary + signature  ------------>   on-device verify (openssl cms / cosign)
                                                        |  gate: execute only if valid
                                                        v
                                                 verified boot (SIGNED_BOOT + dm-verity)
                                                 -- LOCAL ENFORCEMENT, NOT ATTESTATION --

   Layer separation: Vault/YubiKey custody the BUILD-time signing key (off the device).
   The TPM protects the DEVICE-time runtime keys and can hold the verification public key.
   Neither yields remote boot attestation on Raspberry Pi (no measured-boot root of trust).
```

**Key limitation, stated up front.** The Raspberry Pi has no measured-boot root of trust. A bolt-on TPM cannot do boot attestation or remote attestation on this hardware. The correct RPi boot-integrity control is verified (signed) boot plus a read-only rootfs. The TPM's role is key protection. The MCU side does **not** currently close this gap either: the Zephyr/MCUboot answer this report originally cited was retired with the Zephyr tree (ADR [0010](../decisions/0010-mcu-runtime-embassy.md)), and the Embassy/RP2350 firmware that replaced it ships no verified boot chain — see §3. A claim that "RPi + TPM = remote boot attestation" is false for this platform.

---

## 1. Build-Time Binary Signing: The Key Is Never in CI

The on-device signature gate (`distro/deploy/verify-binary.sh`) and any future signed-boot chain both depend on a private signing key. The question is where that key lives when the pipeline signs. This design rejects the weakest posture, a private key stored as a GitHub Actions or CI secret, because any workflow with access can exfiltrate that secret and it is not hardware-bound. Two custody options keep the key off CI and off disk.

### Option A: HashiCorp Vault Transit via cosign (automated CI)

`cosign` already signs release provenance and is baked into the build image. It supports Vault's Transit secrets engine as a KMS backend. The pipeline signs by calling Vault. The key stays in Vault.

```bash
# key ref is hashivault://<transit-key-name>; the transit mount is set via the
# TRANSIT_SECRET_ENGINE_PATH env var (default "transit"), NOT in the URI path
cosign sign-blob --key hashivault://sentient-signing \
  --tlog-upload=false --bundle SHA256SUMS.cosign.bundle SHA256SUMS
```

- **Key custody.** Vault generates and holds the key. The pipeline calls `transit/sign` (ACL: `update` on `transit/sign/sentient-signing`) and receives only the signature. The private key never appears in CI, in a GitHub secret, or on the build host. Transit signs with ECDSA (P-256/384/521) or Ed25519.
- **Keyless CI to Vault auth.** GitHub Actions' OIDC token (`id-token: write`) authenticates to a Vault `auth/jwt` or `auth/oidc` role. `bound_issuer` and `bound_claims` scope it to the repo and ref. Each run mints a short-lived, auto-revoked Vault token. No long-lived Vault credential sits in CI.
- **Offline device verification, with a version requirement.** Signing uses an explicit `--key`, not keyless/Fulcio, so verification needs only the exported public key and no Sigstore infrastructure:
  ```bash
  cosign verify-blob --key sentient-signing.pub \
    --bundle SHA256SUMS.cosign.bundle --insecure-ignore-tlog=true SHA256SUMS
  ```
  Two requirements make this work offline. First, the verify side must also skip the transparency-log check (`--insecure-ignore-tlog=true`), or cosign fails closed waiting for a Rekor inclusion proof. Second, pin and test a specific cosign version: `--tlog-upload=false` was silently not honored in cosign v3.0.2 and the entry still uploaded to the public Rekor log ([sigstore/cosign#4503](https://github.com/sigstore/cosign/issues/4503)), which breaks the air-gap guarantee. Confirm on the pinned version that no Rekor upload occurs before you rely on the offline claim.
- **Assurance tiering.** Vault OSS Transit protects the key in software inside Vault. This beats a CI secret but is not hardware custody. Vault Enterprise with Managed Keys delegates Transit sign and verify to an external HSM over PKCS#11, so the key material lives in the HSM (seal-wrap adds an HSM-encrypted layer over Vault's stored data). Managed Keys and seal-wrap are Enterprise-only.

### Option B: YubiKey 5 FIPS (air-gapped, human-touch)

A YubiKey 5 FIPS holds the signing key on a hardware token. An engineer touches the token to sign each release (PIV slot 9C or an OpenPGP slot, `touch-policy=always`, immutable after key generation). The key never exists on a build machine and the token is never network-reachable. It drops onto the existing detached-signature workflow with no change.

- **Price:** $88–$98 for the USB-A and USB-C SKUs. The 5Ci is $115.
- **FIPS level, precisely.** FIPS 140-3 Overall Level 2 / Physical Security Level 3, Cert #5291, on firmware 5.7.4 or newer (enables NIST SP 800-63B AAL3). Firmware 5.4.x stock is FIPS 140-2 and is on NIST's sunset list. Specify firmware 5.7.4 or newer when procuring.
- **Trade-off.** Signing requires human presence, so a YubiKey does not fit a fully-automated CI release.

### Which One to Use

Use a YubiKey when releases are cut by hand: it gives the strongest custody, since the key is never networked. Use Vault when releases are cut by the pipeline: it signs without a human in the loop. Both keep the key off CI and off disk. A deployment can run Vault for routine automated releases and hold a YubiKey offline root for high-assurance signing.

### One On-Device Change

The device gate today verifies an **X.509 CMS** detached signature (`openssl cms -verify` against the CA), and `openssl` is already present. A YubiKey PIV slot can custody the CMS signer key and issue those same `openssl cms` signatures with **no on-device change**. If instead you custody the key in Vault Transit (or a YubiKey PGP/cosign slot), signing produces a raw ECDSA/Ed25519 (or PGP) signature rather than a CMS structure, which moves the gate from `openssl cms -verify` to `cosign verify-blob --key ...` (or `gpg --verify`) in `verify-binary.sh` — a heavier on-device dependency. That is the trade for a hardware-custodied key and, in the cosign case, one signing tool that spans release provenance and binary signing.

---

## 2. Device-Side TPM (RPi): Key Protection, Not Attestation

### Why a TPM, and Why Not an HSM

The device threat model is one unit, one hardware root of trust. A discrete TPM serves that model directly. Enterprise HSMs (Thales Luna, Entrust nShield, AWS CloudHSM; $1,000–$50,000 appliances or ~$1.60/hr cloud) serve centralized, multi-user PKI roots and exceed what a single field device needs. Put hardware-rooted key custody at fleet scale in a cloud-side CA (§4), not on the RPi.

### Recommended Part: Infineon SLB9672

| | SLB9672 (prefer) | SLB9670 (fallback) |
|---|---|---|
| Board examples | LetsTrust TPM (current product; ~£26/€25 ≈ $26–$33), ANAVI TPM 2.0 (open-source, CrowdSupply, ~mid-2026) | GeeekPi TPM9670 (~$30, US-stocked), generic 9670 HATs |
| RSA / ECC | up to 4096-bit / P-256 + P-384 | up to 2048-bit / P-256 only |
| Firmware update | field-updatable (world-first XMSS PQC-protected FW-update) | limited |
| Temperature | base −20/+85 °C; **−40/+85 (enhanced)** and **−40/+105 (extended)** are variant/special-order | −20/+85 °C |
| Certs | FIPS 140-2 L2; two 2024 firmware-specific CC EAL4+/AVA_VAN.4 | FIPS 140-2 L2; CC EAL4+ |
| Linux driver / overlay | `tpm_tis_spi` / `dtoverlay=tpm-slb9670` | `tpm_tis_spi` / `dtoverlay=tpm-slb9670` |

Two corrections:

- **Overlay.** No separate `tpm-slb9672` overlay exists. Only `tpm-slb9670-overlay.dts` ships, and Infineon documents it as driver-compatible with the 9672. Use `dtoverlay=tpm-slb9670` for both chips. Verify the silicon on the board, not the overlay name.
- **Temperature.** The −40/+105 °C rating is an extended-temp variant, not the stock part. When the enclosure drops below 0 °C or exceeds 85 °C, order the extended variant by part number. A retail HAT does not provide it by default.

**Pi 5.** The Pi 5 routes SPI through the RP1 southbridge. The RST and PIRQ pins reassign versus the Pi 4, and the overlays were tested on the Pi 4 first. Validate the binding on the exact Pi 5 plus kernel plus `rpi-eeprom` image you ship.

### The Three Key-Protection Use Cases

1. **Seal the LUKS data-partition key to the TPM** (`clevis` plus a `tpm2` pin). An SD card or eMMC image pulled from the device does not decrypt off-platform. That is the field-loss threat for a comms module. Without measured boot, the key seals to TPM presence (optionally a TPM auth policy), not to a PCR set that proves boot state. It still defeats "pull the card and read it on another machine."
2. **Hold the mosquitto cloud-bridge TLS client key** as a non-exportable TPM object through `tpm2-pkcs11`, once the bridge moves to mutual TLS. A compromised rootfs cannot copy the key off the box.
3. **Device identity key.** A TPM-resident keypair gives each unit a hardware-bound identity for enrollment and rotation. The private key never touches the filesystem.

All three protect keys: secrets that cannot be stolen off a captured device. None of them attest how the device booted.

### Integration and Effort

- **From C/CLI:** `tpm2-tools` plus the `tss2` stack (tpm2-tss / `libtss2`), `tpm2-pkcs11` for TLS keys, `clevis` (plus `clevis-luks`) for disk sealing.
- **From Rust:** [`tss-esapi`](https://crates.io/crates/tss-esapi) (safe ESAPI bindings, parallaxsecond) is the primary path. The wolfTPM-based alternative is [`tpm2-rs`](https://github.com/openEPC/tpm2-rs), which wraps the wolfTPM C library and needs system wolfSSL/wolfTPM libraries; it is not a pure-crates.io crate, and no crate named `wolftpm` exists. The shipping code does not depend on a TPM.
- **OpenSSL implementation risk.** The mosquitto PKCS#11 path differs by OpenSSL version: `-engine pkcs11` on OpenSSL 1.1, `-provider tpm2` on OpenSSL 3. A mosquitto built on OpenSSL 3 but configured for the engine interface silently fails to use the TPM key. Pin and test the engine-versus-provider path for the target image.
- **Effort, inside the ≤10 h cap:** bring-up 1–2 h, LUKS seal 2–3 h, PKCS#11 key provisioning 2–3 h (defer until mutual TLS is agreed), docs 1–2 h. Total 6–10 h.

### Mandatory: a LUKS Break-Glass Recovery Key

If the data partition seals to the TPM and the TPM (or its state) is lost, the partition is permanently unrecoverable without a recovery key held in secure offline custody. This is a ship-blocker for the LUKS use case, not an optional extra.

---

## 3. The Honesty Boundary: No Measured Boot or Remote Attestation on RPi

- **No CRTM.** The Pi has no Core Root of Trust for Measurement that extends each boot stage into a TPM PCR from reset. The GPU boot-ROM does not measure into a TPM. A bolt-on SPI TPM cannot attest the boot chain on RPi, because there is no reset-anchored measurement for it to record.
- **Verified boot is the correct RPi control.** The EEPROM bootloader supports signed boot (`SIGNED_BOOT=1`, RSA-2048 image signature, public-key hash fused into OTP). Combined with a dm-verity read-only rootfs plus overlayfs, this enforces "boot only trusted software" locally at boot. It enforces; it does not measure and report. The OTP burn is irreversible. Pilot on sacrificial units and guard the signing key.
- **The MCU side is not a fallback for this, today.** An earlier revision of this report pointed at the Zephyr MCU path as the place where real reset-anchored secure boot existed (MCUboot via sysbuild + imgtool, confirm/revert). That path was retired by ADR [0010](../decisions/0010-mcu-runtime-embassy.md) and the Embassy/RP2350 firmware that replaced it has **no equivalent**: the only signing in the tree is a picotool IMAGE_DEF **demo** signature over a manually-combined image, which is load-time integrity, not a fused root of trust. The RP2350 silicon is capable of OTP-fused secure boot; this build does not use it, and a real bootloader is Gate-0-blocked (#351). Stated plainly so the report does not carry a boot-integrity claim the shipping firmware cannot back — see [`README.md`](./README.md) §5.
- **What this design does not claim.** TPM-based boot attestation or remote attestation on RPi. If remote boot attestation becomes a hard contractual requirement, it requires a platform with a measured-boot root of trust. That is a hardware-selection decision, out of scope for the current RPi module.

---

## 4. Cloud-Side CA (future): YubiHSM 2 FIPS

Deploy this only when Sentient stands up a device-cert CA (issuing client certs to field units) or a TMS data-signing service.

- **Validation.** The current v2.4 is FIPS 140-3 Overall Level 3 (Cert #5302). Cite v2.4 for an unambiguous claim. The earlier v2.2 was FIPS 140-2 Overall Level 2 / Physical-Security Level 3.
- **Specs.** 256 object slots (~128 KB aggregate), RSA 2048–4096, ECC P-256/384/521, AES-128/192/256, HMAC; nano USB-A (12×13×3.1 mm); tamper-evident audit log. Integrates through `yubihsm_pkcs11.so` or the [`yubihsm`](https://crates.io/crates/yubihsm) Rust crate.
- **Cost.** $950 versus a rack-mount HSM at $10k–$50k, which is 1/10 to 1/50 the cost for FIPS 140-3 Level 3.
- **Not for the field device.** It is rated 0–40 °C operating (−20/85 storage). A tactical enclosure runs below 0 °C in cold ops and above 40 °C in solar-heated cabinets, which exceeds that range. Use the extended-temp SLB9672 in the field and the YubiHSM 2 on the climate-controlled server side.

---

## 5. Architecture Fit Matrix

| Use case | SLB9672 TPM (~$30) | YubiKey 5 FIPS ($88–98) | YubiHSM 2 FIPS ($950) | Vault Transit (sw/HSM) |
|---|---|---|---|---|
| Device LUKS disk-key seal | Yes, best (clevis/tpm2) | No | No | No |
| Device TLS client key (non-exportable) | Yes, `tpm2-pkcs11` | No, needs human presence | Conditional, temp + cost | No |
| Device identity / enrollment key | Yes | No | No | No |
| **Build-pipeline binary signing** | No, wrong form factor | Yes, best (air-gapped) | Yes, overkill | Yes, best (automated) |
| Cloud-side CA / device-cert issuance | No | No | Yes, best | Conditional, software unless Enterprise+HSM |
| FIPS 140-3 Level 3 key store | No, 140-2 L2 | No, 140-3 L2 | Yes | Only via Enterprise+HSM |

---

## 6. Validation and Sources

A 6-way research pass fact-checked every load-bearing claim above against vendor and primary sources. Verdicts follow. Corrections are already folded into this report.

### Signing (Vault + cosign)
- **Confirmed.** `hashivault://<keyname>` Transit KMS backend; the private key stays in Vault. [sigstore docs](https://docs.sigstore.dev/cosign/key_management/overview/)
- **Corrected.** Offline verify needs `--insecure-ignore-tlog=true` on the verify side too, and `--tlog-upload=false` was silently ignored in cosign v3.0.2 (still uploaded to Rekor). Pin and test a known-good cosign version. [sigstore/cosign#4503](https://github.com/sigstore/cosign/issues/4503)
- **Confirmed.** GitHub OIDC to Vault `auth/jwt` short-lived token. [GitHub docs](https://docs.github.com/actions/deployment/security-hardening-your-deployments/configuring-openid-connect-in-hashicorp-vault)
- **Confirmed.** Managed Keys and seal-wrap are Enterprise-only; OSS Transit is software key material. [HashiCorp Managed Keys](https://developer.hashicorp.com/vault/docs/enterprise/managed-keys/transit-secret-engine)

### YubiKey 5 FIPS
- **Confirmed.** Price $88–$98 (5Ci $115); on-chip PIV/OpenPGP signing with `touch-policy=always`. [Yubico store](https://www.yubico.com/store/yubikey-5-fips-series/)
- **Corrected.** FIPS 140-3 Overall L2 / Physical L3, Cert #5291, firmware 5.7.4 or newer, not a bare "140-3 validated." Firmware 5.4.x is 140-2 and sunset-listed. [Yubico blog](https://www.yubico.com/blog/yubikey-5-fips-series-is-now-fips-140-3-validated-what-it-means-for-high-assurance-security/)

### TPM silicon
- **Confirmed.** SLB9672 RSA-4096 / P-384 / XMSS FW-update; `tpm_tis_spi` driver. [Infineon product brief](https://www.infineon.com/dgdl/Infineon-Product_Brief_OPTIGA_TPM_SLB_9672-ProductBrief-v01_00-EN.pdf)
- **Corrected.** The −40/105 °C rating is an extended variant; the base is −20/85. AES-192 is optional. [Infineon SLB9672 datasheet FW16.xx]
- **Corrected.** No `tpm-slb9672` overlay exists; `tpm-slb9670` drives both. [RPi kernel overlay source](https://github.com/raspberrypi/linux/blob/master/arch/arm/boot/dts/overlays/tpm-slb9670-overlay.dts)
- **Confirmed.** Pi 5 RP1 southbridge routes SPI; RST/PIRQ reassignment. [Infineon community (Pi 5 SLB9672)](https://community.infineon.com/t5/OPTIGA-TPM/Handling-RST-and-PIRQ-Pin-Reassignments-for-TPM-SLB9672-on-Raspberry-Pi-5/td-p/967484)

### Boards / supply
- **Corrected.** LetsTrust's current product is the SLB9672 (~£26 ≈ $26–$33, EU-stocked). [The Pi Hut](https://thepihut.com/products/letstrust-tpm-for-raspberry-pi)
- **Corrected.** SLB9670 fallback repointed to GeeekPi TPM9670 (~$30, US). The Infineon eval kit is discontinued. [DigiKey (eval, discontinued)](https://www.digikey.com/en/products/detail/infineon-technologies/TPM9672FW1523PCEBTOBO1/22195352)
- **Confirmed.** ANAVI TPM 2.0 is a real OSHW SLB9672 board, shipping ~June 2026. [Crowd Supply](https://www.crowdsupply.com/anavi-technology/anavi-tpm-2-0-for-raspberry-pi)

### YubiHSM 2
- **Confirmed.** $650 / $950; 0–40 °C operating (field-disqualifying); `yubihsm` crate. [Yubico store](https://www.yubico.com/store/yubihsm-2-series/) · [yubihsm crate](https://crates.io/crates/yubihsm)
- **Corrected.** v2.4 is FIPS 140-3 Overall Level 3 (v2.2 was 140-2 L2/phys-L3). [Yubico FIPS 140-3 press release](https://www.yubico.com/press-releases/yubico-achieves-fips-140-3-validation-for-yubihsm-2-fips-strengthening-hardware-root-of-trust-for-critical-infrastructure/)

### RPi boot integrity (§3): all 7 claims confirmed
- **Confirmed.** No CRTM; no measurement into TPM PCRs from reset; therefore no measured boot or attestation on RPi. [tpm.dev boot-with-TPM](https://github.com/tpm2dev/tpm.dev.tutorials/blob/master/Boot-with-TPM/README.md)
- **Confirmed.** `SIGNED_BOOT=1` verified boot (RSA-2048 `boot.sig`, OTP-fused key hash, irreversible), Pi 4/CM4/Pi 5. Local enforcement, not attestation. [RPi usbboot secure-boot](https://github.com/raspberrypi/usbboot/blob/master/docs/secure-boot.md)
- **Confirmed.** dm-verity RO rootfs plus overlayfs is the standard tamper-evident pattern. [kernel dm-verity](https://docs.kernel.org/admin-guide/device-mapper/verity.html)
- **Confirmed (nuance).** `clevis luks bind tpm2 '{}'` (no `pcr_ids`) seals to TPM presence, not boot state. It addresses SD-card theft, not boot tampering. [Ubuntu clevis-tpm2](https://ubuntu.com/server/docs/how-to/security/tpm-backed-luks-decryption-with-clevis/)
- **Confirmed.** `tpm2-pkcs11` non-exportable keys; the OpenSSL 3 engine-to-provider split is a silent-failure risk (noted in §2). [tpm2-pkcs11](https://github.com/tpm2-software/tpm2-pkcs11/issues/592)
- **Confirmed.** A LUKS break-glass slot is mandatory: a TPM-only seal plus a lost TPM equals an unrecoverable volume. [Gentoo TPM/LUKS](https://wiki.gentoo.org/wiki/Trusted_Platform_Module/LUKS)

**Honesty guardrail.** This report keeps "no measurement chain into a TPM" separate from "no boot security." The RPi has OTP-fused verified boot. The report never calls signed boot "measured boot" or "attestation," and it states the LUKS threat model (card theft) explicitly.

### Open Items Requiring a Human Spot-Check Before Customer Delivery
1. **cosign version.** Pin a version and confirm on that version that `--tlog-upload=false` produces no Rekor upload (the v3.0.2 bug). This is load-bearing for the air-gap claim.
2. **YubiHSM 2 and SLB9672 temperature.** Secondary sources corroborate both figures. The primary vendor datasheet PDFs did not machine-parse. Spot-check the two PDFs for a page-level primary cite.
3. **ANAVI TPM 2.0 price.** Crowd Supply blocks automated fetch. Verify the live pledge tier (assumed $25–$35).
4. **Bare-die $5–$8/unit at 100+.** This is an order-of-magnitude estimate. Get a Digi-Key/Mouser SLB9672 die quote plus a JLCPCB/PCBWay PCBA quote to confirm.

---

*Consolidates SPIKE #63 (TPM options) and SPIKE #64 (verifiably-trusted binary). Full posture context: [`README.md`](./README.md).*

# Sentient METEOR — Yocto image hardening checklist

Security hardening is the **compensating control** for moving the Pi shipping target from bare
Zephyr (tiny attack surface, no network stack) to a Yocto Linux image (full kernel + userland,
needed because Zephyr has no Ethernet L2 driver for the Pi — see `CLAUDE.md` § "Platform &
Deployment" and story #159). A minimal Linux image must *earn* an equivalent posture rather than
get it for free. This is the first-image, config-level checklist; see the end of this document for
what is explicitly deferred to a follow-up milestone.

**Base/app split (systemd-sysext).** The shipping artifact is no longer one monolithic image: the
**base** (`sentient-image`) is immutable and contains *no application binary*, and the **app**
(`sentient-app-sysext`, a squashfs `.raw` carrying only `bin-rpi` + its `extension-release`
metadata) is merged into the base's `/usr` at boot by `systemd-sysext.service`, matched via the
`SYSEXT_LEVEL` contract (see `distro/README.md` § "Runtime architecture"). The hardening posture
below applies **per artifact**: image-minimization / kernel / read-only-rootfs rows are properties
of the base; the `.wic` signing row (§8) has a sibling for the app `.raw`. Read-only-rootfs is
**preserved, not weakened**, by the split — the base `/` stays read-only, and the sysext overlay
presents the merged `/usr` as read-only too, exactly as a single-image `/usr` would be.
Merge-time enforcement (verifying the extension is untampered *before* systemd merges it, e.g.
dm-verity or an image-policy gate) is **not yet implemented on-device** — Phase 1 (this document)
relies on the out-of-band CMS signature being checked at build/flash/deploy time only; on-device
merge-enforcement is Phase 2 of the signing model, tracked as part of the broader Phase 3 hardening
milestone below (the dm-verity/RPi-secure-boot bucket) — see §8 and the follow-ups below.

Companion docs: [`docs/security.md`](../docs/security/README.md) (Rust-level/application security
posture — broker binding, TLS bridge, systemd unit, boot-integrity roadmap) and
[`docs/deployment.md`](../docs/deployment/README.md) (operational runbook: flashing, config, mosquitto).
This file is the **Yocto-image-level** checklist that sits alongside those — it does not repeat
their detail, it points at the recipe/config that implements each item and how to verify it.

Each item below is tagged with the file that owns it and **who authored it** — items marked
"(this task)" are the standalone recipe files this task created; everything else (distro conf,
image recipe, systemd unit, mosquitto config, CI) is owned by other workstreams under the same
story #159 and is referenced here, not duplicated.

---

## 1. Image minimization

Biggest single attack-surface win: nothing exploitable if it isn't in the image.

| Item | Owned by | Verify by |
|---|---|---|
| No `debug-tweaks` in `EXTRA_IMAGE_FEATURES` (on by default upstream; enables root login + empty passwords) | `sentient-image.bb` (other workstream) | `bitbake-getvar -r sentient-image IMAGE_FEATURES` shows no `debug-tweaks`; boot the image and confirm root login is refused / no empty password |
| No `package-management` (no opkg/rpm/apt in the running image) | `sentient-image.bb` | `which opkg rpm apt` all fail on the booted image |
| No `ssh-server-*`, `dev-pkgs`, `tools-debug`, `tools-sdk` | `sentient-image.bb` | `bitbake -e sentient-image \| grep IMAGE_FEATURES`; `ss -tlnp` shows no sshd listening |
| `read-only-rootfs` IMAGE_FEATURE (also the prerequisite for dm-verity later) | `sentient-image.bb` | `mount \| grep ' / '` shows `ro`; `touch /testfile` on the booted root fails |
| Root account locked (`inherit extrausers` / `EXTRA_USERS_PARAMS`) | `sentient-image.bb` / distro conf | `passwd -S root` shows locked (`L`); `grep root /etc/shadow` shows `!` or `*` in the password field |
| **Base image ships no application binary at all** — `bin-rpi` moved out of `sentient-image` into the separately-built `sentient-app-sysext` extension (base/app split, systemd-sysext) — so a base-only compromise (before any extension is ever merged) has no agent code to exploit | `sentient-image.bb` (drops `sentient-agent-bin` from its image `RDEPENDS`) / `sentient-app-sysext.bb` (other workstream) | `test ! -e /usr/bin/bin-rpi` on a **freshly booted base with `/var/lib/extensions/` empty**; confirm it reappears only after a signed extension is merged |
| `systemd-sysext` support present + `systemd-sysext.service` enabled (the merge mechanism the app relies on) | `sentient-image.bb` / distro conf (other workstream) | `systemctl is-enabled systemd-sysext.service` → `enabled`; `cat /etc/os-release` shows `SYSEXT_LEVEL=1` |

`# TODO(verify-at-build): confirm the exact EXTRA_USERS_PARAMS lock-root syntax against the pinned
wrynose extrausers.bbclass — the plan flags this as an impl-time recipe caveat, not fabricated
here.`

## 2. Binary / compiler hardening

| Item | Owned by | Verify by |
|---|---|---|
| Distro-wide PIE + full RELRO+BIND_NOW + `-fstack-protector-strong` + FORTIFY for every recompiled OS package | `conf/distro/include/security_flags.inc` (other workstream) | inspect `TARGET_CC_ARCH`/`TARGET_LDFLAGS` via `bitbake -e <any-recipe>`; spot-check a packaged ELF with `checksec --file=<binary>` |
| **`bin-rpi` ASLR (static-PIE) — ACCEPTED RESIDUAL RISK; root cause is a rustc target-capability gap.** Verified 2026-07-12 (`rustc -Zunstable-options --print target-spec-json`, rustc 1.92): the **`aarch64-unknown-linux-musl` target spec lacks `static-position-independent-executables`** (x86_64-musl has it), so **rustc cannot emit static-PIE for aarch64-musl** — a plain build and `-C relocation-model=pic -C link-self-contained=yes` both produce `Type: EXEC` (static **non-PIE → no ASLR**). Not a missing flag; no `RUSTFLAGS` fixes it today. Our binary IS fully static (ABI-safe) — only ASLR is absent. **Path to restore ASLR (follow-up #150/#154):** either (a) a glibc-**dynamic** aarch64 build (PIE by default) compiled against the *image's own glibc* (e.g. via the Yocto SDK / build-from-source in BitBake) so there's no glibc skew, or (b) upstream aarch64-musl static-PIE support. | `aarch64-unknown-linux-musl` build (#150) + `sentient-agent_0.1.0.bb` | **Gate (`just verify-musl` — part of the process):** hard-fails if the binary is not fully static (`readelf -l`/`-d`: no `INTERP`/`NEEDED`); reports `Type: DYN` as OK (ASLR present) or `Type: EXEC` as a loud **accepted-risk WARN** referencing this row — so the gap can never ship silently, and the gate auto-passes to OK if a rustc bump adds aarch64-musl static-PIE. |
| Residual gap: no stable-Rust stack-canary equivalent (`-Z stack-protector` is nightly-only) | accepted, documented in `docs/security.md` | N/A — accepted risk, not a gate |

## 3. systemd sandbox on the agent unit

The primary process-isolation control — re-imposes a Zephyr-like tight boundary around `bin-rpi`
on top of a full Linux kernel.

| Item | Owned by | Verify by |
|---|---|---|
| `NoNewPrivileges`, `ProtectSystem=strict` (+`StateDirectory=`), `ProtectHome`, `PrivateTmp`, `MemoryDenyWriteExecute=yes`, empty `CapabilityBoundingSet`, `RestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX`, `SystemCallFilter=@system-service`, `ProtectKernel*`/`LockPersonality`/`RestrictSUIDSGID` | `recipes-sentient/sentient-agent/files/sentient-agent.service` (other workstream) | `systemd-analyze security sentient-agent.service` — should score in the hardened range |
| Serial access without `PrivateDevices=yes` (which would remove the BMS UART): `DevicePolicy=closed` + explicit `DeviceAllow=/dev/ttyUSB0 rw` (or `/dev/ttyAMA0`, matching the deployed `[serial] port`), **static** `User=sentient`/`Group=sentient` (not `DynamicUser` — a recycled UID would strand ownership of the serial device + `StateDirectory`) | same unit file | confirm `mosquitto_sub`/functional e2e still receives TMS payloads with the sandboxed unit active — a hardening regression here manifests as the serial link silently breaking, not a crash |

## 4. Network / broker hardening

| Item | Owned by | Verify by |
|---|---|---|
| **Default-drop nftables firewall**, no inbound MQTT port opened by default | `recipes-connectivity/nftables/` (**this task**) | `nft list ruleset` on the booted image shows `policy drop` on the `input` chain; `nmap`/`nc` from another host against the device's IP times out on every port except what's deliberately opened |
| nftables systemd service enabled at boot | `nftables_%.bbappend` (**this task**) | `systemctl is-enabled nftables.service` → `enabled`; `systemctl is-active nftables.service` → `active` after boot |
| Embedded rumqttd broker stays loopback-only (`127.0.0.1:1883`, unauthenticated plaintext by design — see `docs/security.md` §1.1-§1.2) — **the actual architecture needs no inbound MQTT ACCEPT rule at all**, since the only network-facing MQTT traffic is the mosquitto bridge's *outbound* connection to the cloud | `lib-mqtt` (`crates/`, unaffected by this work) | `ss -tlnp \| grep 1883` shows the broker bound to `127.0.0.1` only, never `0.0.0.0` |
| mosquitto cloud-relay bridge: TLS + `bridge_insecure false` + single-CA `bridge_cafile` (pinning), `allow_anonymous false` on any listener that IS exposed (none are, by default, in the shipped image) | `recipes-connectivity/mosquitto/files/sentient.conf` (other workstream) | `mosquitto_sub` from an unauthorized host against the device fails; inspect the rendered `cloud-relay.conf` for `bridge_insecure false` |
| **Open gap flagged by another workstream's file** (`recipes-connectivity/mosquitto/files/sentient.conf`'s own TODO): `bridge_tls_version tlsv1.2`-minimum pin and `try_private true` are not yet emitted by `bin-rpi`'s `render_bridge()` (`crates/bin/rpi/src/mosquitto_bridge.rs`) — tracked there, not fixed by this task (out of scope: Rust source change) | `crates/bin/rpi/src/mosquitto_bridge.rs` (Rust workstream) | re-check `render_bridge()`'s emitted fragment once addressed |

`sentient.nft` (this task) ships a commented, LAN-subnet-scoped example rule for the deliberate,
documented exception where an operator widens `[broker] host` off loopback
(`docs/deployment.md` §4) — not enabled by default; never accept `0.0.0.0/0` on that port if it is
ever enabled.

## 5. Kernel hardening (KSPP)

| Item | Owned by | Verify by |
|---|---|---|
| `security` DISTRO_FEATURE + `meta-security` layer pulled in | `conf/distro/sentient.conf` (other workstream) | `bitbake-getvar -r sentient-image DISTRO_FEATURES` includes `security`; `bitbake-layers show-layers` lists `meta-security` |
| Hand-written KSPP config fragment (`FORTIFY_SOURCE`, `STACKPROTECTOR_STRONG`, `STRICT_KERNEL_RWX`, `STRICT_MODULE_RWX`, `RANDOMIZE_BASE` (KASLR), `RANDOMIZE_KSTACK_OFFSET_DEFAULT`, `INIT_ON_ALLOC_DEFAULT_ON`, `SLAB_FREELIST_HARDENED`, `SLAB_FREELIST_RANDOM`, `HARDENED_USERCOPY`, `STRICT_DEVMEM`, no `CONFIG_DEVMEM`, `SECURITY_LOCKDOWN_LSM`, `SECURITY_YAMA`) | `recipes-kernel/linux/files/kspp-hardening.cfg` (**this task**) | diff the merged `.config` against the fragment (`grep -f <(awk -F= '{print $1}' kspp-hardening.cfg) .config`); or run `kernel-hardening-checker` against the built `.config` |
| Unprivileged BPF disabled — this is a **runtime sysctl, not a kernel `.config` symbol**: `kernel.unprivileged_bpf_disabled = 1` | a `systemd-sysctl.d` drop-in (owned by another workstream/base-files, or a documented manual first-boot step — **not** authored by this task, since it is out of scope for the kernel-config fragment) | `sysctl kernel.unprivileged_bpf_disabled` on the booted image reports `1` |
| CI gate: `kernel-hardening-checker` diffing the built `.config` against the KSPP baseline | CI workflow (other workstream, `_yocto.yml`) | CI job fails loudly on a regression |

`# TODO(verify-at-build): confirm every symbol in kspp-hardening.cfg exists (spelling + polarity)
for the pinned Pi kernel version — see that file's own header for detail. Also confirm whether the
`raspberrypi4-64`/`raspberrypi5` MACHINEs select `linux-yocto` or `linux-raspberrypi` as
`PREFERRED_PROVIDER:virtual/kernel`; the bbappend in this task targets `linux-yocto_%.bbappend`
and may need a `linux-raspberrypi_%.bbappend` sibling if the latter is what's actually built.`

## 6. Boot / login branding

Not a security control on its own, but bundled into the same "first-image" checklist since it
lands in the same layer pass.

| Item | Owned by | Verify by |
|---|---|---|
| Branded psplash boot splash | `recipes-graphics/psplash/` (**this task**, wiring only — see `files/README-logo.md` for the real logo-generation step, deferred until a branding asset + Yocto build environment exist) | boot the image under `runqemu` (graphical) with `SPLASH=psplash` + the `splash` feature enabled and confirm the branded logo renders before the login prompt |
| Branded `/etc/issue` (console) + `/etc/issue.net` (network login) | `recipes-core/base-files/` (**this task**) | `cat /etc/issue` on the booted console shows the Sentient METEOR banner + authorized-use notice |
| os-release identity (`DISTRO_NAME`/`DISTRO_VERSION`/`VERSION_ID`) | `conf/distro/sentient.conf` (other workstream — **not** touched by this task) | `cat /etc/os-release` shows `Sentient METEOR` identity |
| Agent-ready banner (systemd `ExecStartPost`/`sd-notify`-tied console message once preflight passes) | `sentient-agent.service` (other workstream) | watch the console/journal at boot for the branded "ready" line once `notify_ready()` fires |

## 7. CVE scanning + SBOM

| Item | Owned by | NIST | Verify by |
|---|---|---|---|
| **wrynose (6.0) removed `cve-check.bbclass`** (OE-Core `00de455f8d3a`), replaced by the SBOM-integrated flow: `create-spdx` (SPDX v3 SBOM) + `vex` (VEX manifest) + the out-of-build **`sbom-cve-check`** tool. Scans the Yocto system layer (kernel/openssl/busybox/mosquitto) against NVD; complementary to `cargo audit`/`deny` (Rust crate graph only) — both needed. | CI / `sentient.conf` (other workstream) | SU-1.2 | `INHERIT += "create-spdx vex"` produces `*.rootfs.spdx.json` + `*.rootfs.json`; then out-of-build: `sbom-cve-check --sbom-path *.rootfs.spdx.json --yocto-vex-manifest *.rootfs.json --export-type yocto-cve-check-manifest --export-path sbom-cve-checked.json`. CI gate: non-zero exit on unapproved HIGH/CRITICAL lacking a VEX justification. |
| SPDX SBOM published as a release artifact | `_yocto.yml` release wiring (other workstream) | SU-1.2 | release artifacts include an `.spdx.json` alongside the `.wic` |

`# TODO(wrynose-cve — impl once base build is green): wire INHERIT += "create-spdx vex". VERIFY the
exact scope of the sbom-cve-check class first — it was found under meta/classes-recipe/ (recipe
scope), so a plain global INHERIT may fail like cve-check did; the working shape is likely
create-spdx + vex (global) + the sbom-cve-check CLI run out-of-build in CI. CVE_CHECK_IGNORE was
retired with cve-check — migrate any suppressions to CVE_STATUS (in-recipe) / VEX statements.`

## 8. `.wic` + app-sysext signing (Phase 1: out-of-band CMS)

The base and the app are now two independently signed artifacts (see the base/app split note
above) — each gets its own detached signature over the same X.509 CMS chain, so the split does not
weaken the existing signing story, it duplicates it onto the second artifact.

| Item | Owned by | Verify by |
|---|---|---|
| Sign the `.wic` **base** release artifact via the existing X.509 CMS + cosign flow (same chain as `bin-rpi` binaries) | `production.yml` `yocto` job (other workstream) | `openssl cms -verify -binary -content sentient-image-<machine>.wic.bz2 -in sentient-image-<machine>.wic.bz2.p7s -inform DER -CAfile ca.pem` succeeds (already documented in `docs/deployment.md` §3) |
| Sign the `sentient-app-sysext` `.raw` **app extension** artifact via the same X.509 CMS chain | `production.yml` `yocto` job (other workstream) — `# TODO(verify-at-build): confirm the exact job step name / artifact glob once the sysext build lands in production.yml; not owned or seen by this doc pass` | `openssl cms -verify -binary -content sentient-app-sysext-<machine>.raw -in sentient-app-sysext-<machine>.raw.p7s -inform DER -CAfile ca.pem` succeeds (same verb as the `.wic` check, applied to the `.raw`) |

**Important distinction — do not conflate:** these sign the **download/flash/deploy artifacts**,
verified *off-device* before flashing (the base) or before an operator drops a new extension into
`/var/lib/extensions/` (the app). This is **NOT on-device verified boot or on-device merge
enforcement** — a compromised device could still be made to boot a tampered rootfs, or
`systemd-sysext` could still be made to merge a tampered `.raw`, if physical/supply-chain access
were obtained after flashing/deployment and the CMS check was skipped or never run. On-device
verified boot (dm-verity + RPi secure boot) and on-device merge-time enforcement of the extension's
signature (e.g. dm-verity on the sysext image, or a systemd image-policy gate) are both **Phase 2**
of the signing model — tracked in the Follow-ups below, inside the broader Phase 3 hardening
milestone bucket. Phase 1, shipped now, is CMS-verify-before-deploy only, on both artifacts.

---

## Follow-up hardening milestone (higher effort — Phase 3, tracked, not built now)

These are **deliberately deferred**, not overlooked — do not treat their absence as a first-image
gap:

- **dm-verity immutable/verified rootfs** (`dm-verity-img.bbclass`; needs the read-only rootfs +
  an initramfs + a root-hash flow) — detects any block-level tampering of the rootfs at read time.
  Post-split, this needs a root hash on **both** artifacts: the base rootfs, and the app
  `sentient-app-sysext` `.raw` — the latter is what closes the Phase 1→2 gap noted in §8 (today
  `systemd-sysext` merges any `.raw` found in `/var/lib/extensions/` with no on-device signature or
  hash check at merge time; the CMS check is only ever performed off-device, before deploy).
- **RPi secure boot**: Pi 4 OTP `SIGNED_BOOT` (RSA-2048, fused key hash — **OTP burn is
  IRREVERSIBLE**, pilot on sacrificial units first) or Pi 5 `rpi-sb-provisioner`, plus a U-Boot
  FIT-signed kernel chain.
- **IMA/EVM file appraisal** (`meta-integrity`).
- **Optional TPM/PARSEC-backed MQTT key custody.**
- **meta-zephyr / `BB_MULTICONFIG` convergence** (folding the MCU firmware build into the same
  BitBake invocation) — purely a build-pipeline consolidation, not a security item, but tracked
  alongside these in story #159's Phase 3.

## Residual risk (accepted, documented — not remediated by any of the above)

**The Raspberry Pi's closed-source VideoCore/RP1 BootROM means a fully open, independently
auditable root of trust is not achievable on this hardware**, even with dm-verity + RPi secure
boot layered on top (those verify *from* the first-stage bootloader onward; the very first stage
lives in a proprietary, unauditable mask ROM). This is a real posture delta versus the Zephyr
baseline this hardening pass compensates for, and is accepted for this platform rather than papered
over — see `CLAUDE.md` § "Platform & Deployment" and `docs/security.md` §2 for the fuller
measured-boot/attestation discussion. If remote boot attestation is ever a hard contractual
requirement, it needs a different hardware root of trust; that is a hardware-selection decision,
out of scope for this Yocto layer.

---

## Quick verification summary

```sh
# On a booted image (Pi hardware or runqemu):
systemd-sysext status                                # confirm the app extension is merged (base/app split)
cat /usr/lib/extension-release.d/extension-release.*  # confirm the merged extension's SYSEXT_LEVEL matches
systemd-analyze security sentient-agent.service      # systemd sandbox score
readelf -h /usr/bin/bin-rpi                          # confirm DYN (static-PIE) — present only post-merge
ldd /usr/bin/bin-rpi                                 # confirm "not a dynamic executable" (musl-static)
nft list ruleset                                     # confirm default-drop input, no inbound MQTT by default
ss -tlnp                                              # confirm no ssh, broker bound to 127.0.0.1 only
cat /etc/issue                                        # confirm branding banner
cat /etc/os-release                                   # confirm distro identity + SYSEXT_LEVEL (owned by another workstream)
mount | grep ' / '                                    # confirm read-only rootfs (base, and merged /usr)
passwd -S root                                        # confirm root is locked
sysctl kernel.unprivileged_bpf_disabled                # confirm 1 (runtime sysctl, not a kernel .config symbol)
kernel-hardening-checker                               # diff running .config against the KSPP baseline
```

```sh
# At build/CI time (wrynose 6.0+ — cve-check removed, now SBOM+VEX):
sbom-cve-check --sbom-path sentient-image-<machine>.rootfs.spdx.json \
  --yocto-vex-manifest sentient-image-<machine>.rootfs.json \
  --export-type yocto-cve-check-manifest --export-path sbom-cve-checked.json   # CVE scan of the system layer
openssl cms -verify -binary -content sentient-image-<machine>.wic.bz2 \
  -in sentient-image-<machine>.wic.bz2.p7s -inform DER -CAfile ca.pem   # verify the signed BASE flash artifact (off-device only)
openssl cms -verify -binary -content sentient-app-sysext-<machine>.raw \
  -in sentient-app-sysext-<machine>.raw.p7s -inform DER -CAfile ca.pem   # verify the signed APP extension artifact (off-device only, before it is deployed to /var/lib/extensions/)
```

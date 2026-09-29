# `distro/` — Yocto/BitBake content for the Raspberry Pi image

This folder holds **all** Yocto Project / BitBake content used to build the Raspberry Pi
shipping image. It is deliberately kept separate from the rest of the repo so the three
build worlds never mix:

| Folder | What it builds | Toolchain |
|---|---|---|
| `crates/` | The Rust workspace — `bin-rpi`, `impl-app`, `lib-*` | `cargo` / `rustc` |
| `crates/bin/mcu-rp2350/` | The MCU firmware (Cortex-M33 / RP2350, Embassy — the Zephyr `bin/zephyr/` tree is retired, see `docs/decisions/0010-mcu-runtime-embassy.md`) | `cargo` + `thumbv8m` (`just mcu-rp2350`) |
| **`distro/`** (this folder) | The Raspberry Pi **Linux** image that *runs* the prebuilt `bin-rpi` binary | `bitbake` (Yocto / Poky) |

## Why a Yocto image at all

The Pi shipping target moved from bare Zephyr to a minimal **Yocto Linux** image (systemd
init, no package manager, hardened by default) because Zephyr has no Ethernet L2 driver
for the Pi 5 today — see `CLAUDE.md` § "Platform & Deployment" for the full rationale.
Yocto does **not** replace the Rust binary or its build: `bin-rpi` is still built exactly
as today (`cargo build --release -p bin-rpi --target aarch64-...`) and is consumed here as
a **prebuilt artifact** — BitBake never invokes `cargo` or a Rust toolchain. The MCU stays
on Zephyr unchanged.

## Runtime architecture — base image + app sysext extension

The base image and the app are **two separately built, separately versioned, separately
signed artifacts**, merged together at boot rather than baked into one rootfs:

```
sentient-image (base, immutable)          sentient-app-sysext (app, immutable)
  systemd 259.5, sysext ENABLED             squashfs .raw, contains ONLY:
  systemd-sysext.service ENABLED              /usr/bin/bin-rpi
  /etc/os-release:                             /usr/lib/extension-release.d/
    ID=sentient                                  extension-release.sentient-app
    VERSION_ID=1.0.0                          extension-release.*:
    SYSEXT_LEVEL=1                               ID=sentient
  NO bin-rpi baked in                           SYSEXT_LEVEL=1
                                                  ARCHITECTURE=arm64
                                                  EXTENSION_RELOAD_MANAGER=1
         └────────────── merged at boot ──────────────┘
                  (systemd-sysext, BEFORE basic.target)
                              │
                              ▼
                    running system's /usr
                 (base /usr + bin-rpi overlaid,
                    read-only, exactly like a
                     single-image /usr would be)
```

- **The base (`sentient-image`) ships with no application binary at all.** It boots with
  `systemd-sysext.service` enabled, which — before `basic.target` — scans
  `/var/lib/extensions/` (a persistent, writable data-partition path, distinct from the
  read-only rootfs) for any `.raw` extension image and merges it into `/usr` via an overlay.
  With no extension present the base still boots (no agent runs); with the app extension
  present, `/usr/bin/bin-rpi` and the `sentient-agent.service` unit it needs are exactly as
  if they'd been baked in.
- **The app (`sentient-app-sysext`) is built by a separate recipe** (`sentient-agent-bin`
  packages the prebuilt `bin-rpi`; `sentient-app-sysext` wraps it as a minimal squashfs
  image) and deployed independently of the base: dropping a new signed `.raw` into
  `/var/lib/extensions/` and triggering a merge (refresh or reboot) updates the running
  application **without re-flashing or rebuilding the base image**.
- **They are matched at merge time by `SYSEXT_LEVEL`**, per the systemd-sysext contract: the
  base `/etc/os-release` declares `SYSEXT_LEVEL=1`; the extension's
  `extension-release.<name>` file must declare the same `SYSEXT_LEVEL=1` (alongside
  `ID=sentient`, `ARCHITECTURE=arm64`, `EXTENSION_RELOAD_MANAGER=1`) or systemd refuses to
  merge it. This is the versioning seam between the two images — the base and the app can
  each move forward independently as long as `SYSEXT_LEVEL` stays in step (or is bumped in
  lockstep on a breaking base/app ABI change).
- **Signing is currently out-of-band (Phase 1):** the `.raw` is CMS-signed the same way the
  `.wic` base image is (see `docs/deployment.md` and `distro/HARDENING.md` §8) — a
  build/flash-time provenance gate, not an on-device enforcement mechanism. On-device
  merge-time verification (dm-verity / an image-policy gate on the extension) is a tracked
  Phase-2 follow-up; see `distro/HARDENING.md`.
- **Read-only rootfs is preserved end to end.** The base `/` stays read-only
  (`read-only-rootfs`, see `distro/HARDENING.md` §1); the sysext overlay presents the merged
  `/usr` as read-only too — this is not a return to a writable/mutable image, just a
  two-artifact composition of one.

## Layout

```
distro/
  meta-sentient/                      # our own Yocto layer (the only layer we own)
    conf/layer.conf                   # layer registration (BBFILE_COLLECTIONS, priority, ...)
    conf/distro/sentient.conf         # custom DISTRO: branding, os-release, hardening flags
    conf/distro/include/              # included .inc fragments (e.g. security_flags.inc)
    recipes-sentient/
      sentient-agent/                 # sentient-agent-bin: packages the prebuilt bin-rpi + hardened
                                       #   systemd unit; consumed by sentient-app-sysext below, NOT
                                       #   baked into sentient-image directly
      sentient-app-sysext/            # the app sysext image recipe — squashfs .raw containing only
                                       #   /usr/bin/bin-rpi + extension-release.d metadata
      sentient-image/                 # the BASE image recipe (core-image-minimal + systemd, sysext
                                       #   enabled) — no longer contains bin-rpi; see "Runtime
                                       #   architecture" above
    recipes-connectivity/
      mosquitto/                      # mosquitto bridge config (TLS-hardened)
  # Pinned upstream layers (poky, meta-openembedded, meta-raspberrypi, meta-security) at the
  # wrynose (5.0 LTS) release are NOT vendored here — they are fetched by the builder image /
  # CI via a manifest (kas or repo/west-style), so this repo stays small. See ci/ (added in the
  # CI/release phase of this work) for exactly how they're pinned and fetched.
```

## What this is NOT

- Not a place to build or vendor Rust code. If you're editing `bin-rpi` logic, you want
  `crates/bin/rpi/`.
- Not the MCU firmware. If you're editing the MCU app or its drivers, you want
  `crates/bin/mcu-rp2350/` (Embassy; the Zephyr `bin/zephyr/` tree was retired — see
  `docs/decisions/0010-mcu-runtime-embassy.md`).
- Not locally buildable without a full Yocto/BitBake environment (multi-GB downloads +
  sstate cache). The intended build environment is the `sentient-yocto-builder` container
  image (added alongside the CI workflow).

## Where things run

```
Seplos BMS ──RS-485──▶ bin-rpi (sentient-agent-bin, merged in at boot ──▶ embedded rumqttd (127.0.0.1:1883)
                        via the sentient-app-sysext extension)                    │
                                                                          mosquitto bridge (TLS) ──▶ cloud
```

See `docs/deployment.md` for the full runtime picture (this folder only controls how the base image
and the app sysext extension — plus the mosquitto config — are each built; see "Runtime architecture"
above for how they compose at boot into a running Pi).

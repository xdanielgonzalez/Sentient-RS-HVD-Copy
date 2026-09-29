<p class="eyebrow">Deployment · Runbook</p>

# Sentient METEOR adapter — deployment guide

Production deployment of the `bin-rpi` binary on a Raspberry Pi comms module: poll a Seplos
BMS over RS-485, map to TMS (MIL-STD-3071), publish onto the on-device MQTT broker, and relay to
the private cloud via a mosquitto bridge. Companion docs: [`security.md`](../security/README.md) (boot
integrity, TPM, TLS) and [`tms-compliance.md`](../compliance/README.md) (schema shape vs the standard).

**The Pi ships as a minimal Yocto Linux image** (`core-image-minimal` + systemd, for Pi 4-64 and
Pi 5), built and deployed as **two separate, independently signed artifacts** rather than one
monolithic image:

- the **base image** (`sentient-image`, flashed as a `.wic`) — systemd, mosquitto, the hardening,
  the firewall — but **no `bin-rpi` baked in**; and
- the **app extension** (`sentient-app-sysext`, a squashfs `.raw`) — just `/usr/bin/bin-rpi` plus
  the `systemd-sysext` `extension-release` metadata that lets the base merge it in.

The base boots with `systemd-sysext.service` enabled, which merges any extension found in
`/var/lib/extensions/` into `/usr` **before `basic.target`** — so once the app `.raw` is deployed
there, the running system looks exactly as if `bin-rpi` had been baked into the base all along. You
**flash the base `.wic` image and deploy the app `.raw` separately**; you do not hand-install
packages onto Raspberry Pi OS. The `distro/meta-sentient` Yocto layer (see
[story #159](https://github.com/High-Velocity-Decisions/sentient-rs/issues/159)) is the source of
truth for what lands on the device; §3 below covers flashing the base and deploying the app
extension, and §4–§8 describe the on-device config/behaviour the layer bakes in (still the
operational reference for tuning a running unit). This supersedes the earlier "Zephyr on the Pi"
plan — the Pi runs Linux so its kernel drives Ethernet and the tokio/`std::net` path works; the MCU
stays on Zephyr.

**Phase 1 signing is out-of-band CMS on both artifacts** (the same X.509 CMS chain, applied twice —
see §3): a build/flash/deploy-time provenance gate, not on-device enforcement. On-device
merge-time verification of the extension (dm-verity or an image-policy gate, so a tampered `.raw`
can't be merged even if it lands in `/var/lib/extensions/`) is a tracked Phase-2 follow-up — see
`distro/HARDENING.md` §8 and its Follow-up hardening milestone.

---

## 1. What runs on the device

```
Seplos BMS ──RS-485──▶ bin-rpi ──in-process──▶ embedded rumqttd broker (127.0.0.1:1883)
                        (poll→decode→TMS→publish)              │
                                                        mosquitto bridge (subscribes tms/#)
                                                               │  TLS
                                                        private-cloud MQTT endpoint
```

- `bin-rpi` — the deliverable binary. Hosts the broker and runs the poll loop.
- `mosquitto` — the relay. Subscribes to the device broker over loopback and bridges `tms/#` out
  to the cloud with TLS. **The device never pushes to the cloud from Rust; mosquitto pulls.**

---

## 2. RS-485 wiring

The Seplos pack speaks RS-485 at **19200 8N1**, ASCII `~`-framed request/response.

| Signal | Notes |
|---|---|
| A / A+ (D+) | RS-485 non-inverting. Match device A↔adapter A. |
| B / B− (D−) | RS-485 inverting. Swap A/B if you get only timeouts. |
| GND | Common ground between the Pi's RS-485 adapter and the BMS. |
| Termination | 120 Ω across A/B at both physical ends of the bus only. A short bench run usually needs none. |

- **DIP addressing:** each pack has a DIP-set address 0–15; the poll list is `poll.dip_addresses`
  in config. A single pack is DIP 0.
- **Adapter options:** a USB-RS485 dongle (appears as `/dev/ttyUSB0`) or a Pi HAT/GPIO-UART on
  `/dev/ttyAMA0`. For half-duplex direction control on a `rs485` port the driver toggles RTS; a
  dongle with automatic direction control also works (use `kind = "rs232"` if it needs no toggle).

### 2.1 Waveshare RS485 CAN HAT setup

**PROVISIONAL** — no reference deployment (`/home/user/meteor-adapter` was absent in the
environment this section was authored in — see CLAUDE.md's reference-prototype policy) was
available to confirm these exact `config.txt` lines/wiring against the specific HAT this fleet
ships on. This is a best-effort derivation from the Waveshare RS485 CAN HAT's publicly documented
GPIO-UART wiring (the named HAT the gap analysis flagged as undocumented — see
`docs/gap-analysis-meteor-parity.md`'s "Waveshare HAT setup docs" finding) plus the standard
Raspberry Pi OS UART-overlay mechanism. **Verify every line below against the actual shipping HAT
before treating it as load-bearing field guidance** — this is the same "verify on the shipping
HAT" caveat decision 0005 (`docs/decisions/0005-rs485-direction-mechanism.md`) already carries for
the direction-control mechanism itself.

The Waveshare RS485 CAN HAT talks to the Pi over the **primary PL011 UART on the 40-pin header**
(GPIO14/TXD, GPIO15/RXD), not USB — so it needs that UART freed from two things that claim it by
default on Raspberry Pi OS/most Yocto BSPs: the Linux console (which otherwise gets a login shell
on it) and, on Pi models with onboard Bluetooth (Pi 3/4/Zero W and later), the Bluetooth modem
(which the firmware maps onto the good, fixed-clock `ttyAMA0` PL011 by default, demoting UART to
the lower-quality mini-UART `ttyS0` whose clock tracks the CPU frequency — unacceptable jitter for
a fixed-baud RS-485 poll).

Add to `/boot/config.txt` (or `/boot/firmware/config.txt` on newer Raspberry Pi OS layouts; the
Yocto `distro/meta-sentient` boot partition — see `distro/HARDENING.md`):

```ini
# Free the primary PL011 UART (ttyAMA0) from Bluetooth so the Waveshare HAT gets the full-quality,
# fixed-clock UART instead of the CPU-clock-tracking mini-UART.
dtoverlay=disable-bt

# Also disable the serial console getty on ttyAMA0 (raspi-config's "Serial Port" -> off does the
# same); otherwise a login shell fights the RS-485 poller for the same device.
enable_uart=1
```

Then mask the console service that would otherwise grab the freed UART:

```sh
sudo systemctl disable serial-getty@ttyAMA0.service
sudo systemctl mask serial-getty@ttyAMA0.service
```

**If the primary UART is already spoken for** (e.g. Bluetooth is wanted, or another HAT/peripheral
already owns `ttyAMA0`), the Pi 4/5's GPIO header exposes several additional PL011-backed UARTs via
overlay, each on a different GPIO pair — `dtoverlay=uart2` (GPIO0/1), `dtoverlay=uart3` (GPIO4/5),
`dtoverlay=uart4` (GPIO8/9), `dtoverlay=uart5` (GPIO12/13). Move the Waveshare HAT's RXD/TXD
jumpers (or wiring, on the header-mounted variant) to the overlay's GPIO pair and add the matching
line instead of/alongside `disable-bt` — this is also the mechanism for wiring a **second** bus
(multi-port, decision 0004 — `docs/decisions/0004-multi-port-in-process.md`): one physical UART per
configured `[[bus]]`.

**Which `/dev/tty*` appears:**

| `config.txt` overlay | Device node | Notes |
|---|---|---|
| `dtoverlay=disable-bt` (default primary UART, GPIO14/15) | `/dev/ttyAMA0` | The full-quality PL011; what `disable-bt` frees from Bluetooth. |
| `dtoverlay=uart2` (GPIO0/1) | `/dev/ttyAMA2` | |
| `dtoverlay=uart3` (GPIO4/5) | `/dev/ttyAMA3` | |
| `dtoverlay=uart4` (GPIO8/9) | `/dev/ttyAMA4` | |
| `dtoverlay=uart5` (GPIO12/13) | `/dev/ttyAMA5` | |
| No overlay, Bluetooth still enabled (Pi 3/4/Zero W+) | `/dev/ttyS0` | The mini-UART fallback — do not use for RS-485; its baud tracks the CPU clock and drifts under load. |

Point `[serial].port` (or the relevant `[[bus]].serial.port`) at whichever node the chosen overlay
produces, e.g.:

```toml
[serial]
kind = "rs485"
port = "/dev/ttyAMA0"    # dtoverlay=disable-bt, the HAT's default wiring
baud = 19200
```

**Wiring** (HAT screw terminals to the Seplos pack — same signal meaning as §2's table above): A/A+
to A/A+, B/B− to B/B−, GND to GND. The HAT's onboard transceiver handles DE/RE itself from the
UART's RTS line by default (`direction = "rts"`, this project's default — see decision 0005); a
board with a jumper-selectable auto-direction mode may not need RTS toggling at all, in which case
`kind = "rs232"` (no toggle) is the correct config even though the physical layer is still
RS-485 — see §2's adapter-options note.

---

## 3. Flash the base image, then deploy the app extension

Two separate steps now, matching the two separate artifacts (see the architecture summary above):
**(a)** flash the signed base `.wic` — a one-time (or rare) step, same as flashing any Yocto image —
and **(b)** deploy the signed app `.raw` into `/var/lib/extensions/`, which is the step you repeat on
every application update (no re-flash needed).

### 3a. Flash the base image

The base deliverable is a signed Yocto **`.wic`** image (built by the `distro/meta-sentient` layer in
CI — see the release artifacts) with mosquitto, the systemd units (including `systemd-sysext.service`,
enabled), the firewall, and the hardening — but **no `bin-rpi`**. Flash it to the boot medium:

```sh
# Pick the artifact for the board: sentient-image-raspberrypi5.wic.bz2 (Pi 5)
#                                   sentient-image-raspberrypi4-64.wic.bz2 (Pi 4, 64-bit)
# Verify the signature first (same X.509 CMS + cosign chain as the bin-rpi artifacts — see security.md).
# ca.pem is attached to the release; the .wic.bz2 carries a detached CMS signature .p7s:
openssl cms -verify -binary -content sentient-image-raspberrypi5.wic.bz2 \
  -in sentient-image-raspberrypi5.wic.bz2.p7s -inform DER -CAfile ca.pem

# Flash to the SD card / storage (bmaptool is preferred; plain dd also works):
bunzip2 -k sentient-image-raspberrypi5.wic.bz2
sudo dd if=sentient-image-raspberrypi5.wic of=/dev/sdX bs=4M conv=fsync status=progress
```

The image signature authenticates the **flash artifact** (verified off-device before flashing); it is
distinct from on-device verified boot (dm-verity / RPi secure boot), which is a tracked hardening
follow-up (see [story #159](https://github.com/High-Velocity-Decisions/sentient-rs/issues/159)).

A base booted with nothing in `/var/lib/extensions/` comes up with systemd + mosquitto but **no
`bin-rpi` and no `sentient-agent.service`** — it is not yet a functioning device until §3b lands the
app extension.

### 3b. Deploy the app extension

The app deliverable is a signed **`sentient-app-sysext-<machine>.raw`** — a small squashfs image
containing only `/usr/bin/bin-rpi` and the `systemd-sysext` `extension-release` metadata that
identifies it (`ID=sentient`, `SYSEXT_LEVEL=1`, `ARCHITECTURE=arm64`,
`EXTENSION_RELOAD_MANAGER=1`). Deploy it to the device's writable data area and (re)merge:

```sh
# Verify the signature first — same CMS chain and CA as the base image:
openssl cms -verify -binary -content sentient-app-sysext-raspberrypi5.raw \
  -in sentient-app-sysext-raspberrypi5.raw.p7s -inform DER -CAfile ca.pem

# Copy it into the persistent, writable extensions directory on the device
# (over SCP/USB/whatever your provisioning transport is — the base ships no SSH server, see
# distro/HARDENING.md §1, so this step assumes local/physical access or an existing transport):
sudo mkdir -p /var/lib/extensions
sudo cp sentient-app-sysext-raspberrypi5.raw /var/lib/extensions/sentient-app.raw

# Merge it in — either explicitly (no reboot needed) or implicitly on next boot:
sudo systemd-sysext refresh
# — or —
sudo reboot
```

`systemd-sysext.service` (enabled in the base) also auto-merges everything under
`/var/lib/extensions/` at every boot, before `basic.target` — so a reboot alone is sufficient; the
explicit `refresh` just avoids waiting for one. Updating the app in place (new binary, same base) is
the same two commands: overwrite `/var/lib/extensions/sentient-app.raw` with the new signed `.raw`
and `systemd-sysext refresh` again. See §3c to verify the merge succeeded.

**The extension-release contract:** the base `/etc/os-release` and the extension's
`extension-release.<name>` file are matched on `SYSEXT_LEVEL` (both `1` today) — if they diverge,
`systemd-sysext` refuses to merge. This is what lets the base and the app version independently: a
base bump that doesn't change `SYSEXT_LEVEL` still accepts the existing app extension unmodified.

**Phase 1 signing is out-of-band only:** the CMS check above is a *provenance gate the operator (or
CI/flash tooling) runs before copying the file into place* — `systemd-sysext` itself does not verify
the `.raw`'s signature at merge time. Do not skip the `openssl cms -verify` step; there is no
on-device backstop for it yet (dm-verity / an image-policy gate on the extension is a tracked
Phase-2 follow-up — see `distro/HARDENING.md` §8).

### 3c. Verify the merge

```sh
systemd-sysext status                                  # shows the extension merged + its hierarchy
cat /usr/lib/extension-release.d/extension-release.*    # confirm SYSEXT_LEVEL / ID match the base
cat /etc/os-release                                     # confirm the base's own SYSEXT_LEVEL / ID
systemctl is-active sentient-agent.service              # confirm the app's unit is actually running
```

`systemd-sysext status` reports the merge state and which extension(s) contributed to `/usr`; an
empty/absent result here (with `sentient-agent.service` failing to find its binary) means the `.raw`
either wasn't copied to `/var/lib/extensions/`, wasn't refreshed/rebooted after copying, or failed
the `SYSEXT_LEVEL` match against the base.

### Building the artifacts yourself

Building the base image yourself (needs the `sentient-yocto-builder` toolchain image):
`just yocto-image`. Building the app extension: `kas build distro/kas/sentient.yml --target
sentient-app-sysext`. The `bin-rpi` binary itself is still cross-compiled by the Rust pipeline (as a
static `aarch64-unknown-linux-musl` artifact), staged into the `sentient-agent-bin` recipe's files
directory, and packaged into the `sentient-app-sysext` image — you never hand-build it for the
device.

Per-site **configuration** (`/etc/sentient/sentient.toml`, cloud credentials) is layered on at
provisioning time, not baked into either artifact — see §4–§5.

---

## 4. Configuration

`/etc/sentient/sentient.toml` (TOML), overridable by `SENTIENT_`-prefixed environment variables
(nested with `__`, e.g. `SENTIENT_SERIAL__PORT=/dev/ttyUSB0`). `SENTIENT_CONFIG` selects the file.

```toml
# Top-level RS-485 direction fallback (a bare key, not a [section] — see the note below).
# direction = "rts"            # "rts" | "kernel"/"tiocsrs485" | "gpio" — process-wide default,
#                               # overridden by any [serial]/[[bus]].serial.direction that is set

[device]
id = "METEOR-001"            # published in the TMS topic path and payload identity

[serial]
kind = "rs485"               # "rs485" | "rs232"
port = "/dev/ttyAMA0"        # or /dev/ttyUSB0 for a USB dongle
baud = 19200                 # Seplos default 8N1
parity = "none"
stop_bits = 1
read_timeout_ms = 1000       # per-exchange read deadline; a silent bus -> Timeout, not a hang
# --- RS-485 driver-enable (DE/RE) mechanism (per-port; #245, decision 0005). All optional; shown
# at their defaults. Ignored for kind = "rs232".
# direction = "rts"           # "rts" (software RTS+drain, default) | "kernel"/"tiocsrs485" | "gpio"
# gpio_chip = "gpiochip4"     # required when direction = "gpio"; bare name or "/dev/gpiochip4"
# gpio_line = 22              # required when direction = "gpio"
# gpio_active_high = true     # whether asserting DE drives gpio_line high (default) or low
# settle_tx_us = 5000         # delay after asserting DE, before writing (rts/gpio only)
# settle_rx_us = 10000        # delay after the write completes, before deasserting DE
# tx_drain = true             # flush + tcdrain before dropping DE (rts only; default true)

[poll]
dip_addresses = [0]          # one entry per pack DIP address
interval_ms = 2000           # cycle cadence
fault_every = 10             # poll the 0x44 fault command every N cycles (0 = never)

[broker]
host = "127.0.0.1"           # loopback by default — only the local mosquitto bridge reads it
port = 1883

[publish]
prefix = "tms"               # topic = {prefix}/{device_id}/{snake_case_topic}

# --- Multi-port [[bus]] array-of-tables (decision 0004) — the N-port alternative to the single
# [serial]/[poll] pair above. One entry per physical bus, each with its own name/serial/poll/
# protocol, all sharing the one embedded broker. Mutually exclusive with [serial]/[poll] above
# (Config::buses fails loudly if both or neither are present) — do not uncomment this alongside
# the [serial]/[poll] sections; remove those first.
# [[bus]]
# name = "pack-a"                  # free-text label for logs/taps/watchdog naming
# protocol = "seplos"               # "seplos" (default, RS-485 battery) | "voltronic" (RS-232 inverter)
# device_id = "METEOR-001"          # optional; overrides the derived default ({device.id} for a
#                                    # seplos bus, {device.id}-inv for a voltronic bus) — duplicate
#                                    # device_ids across buses are a config-load error
#
# [bus.serial]
# kind = "rs485"
# port = "/dev/ttyAMA0"
# baud = 19200
# read_timeout_ms = 1000
# # direction/gpio_*/settle_*_us/tx_drain accepted here exactly as under the top-level [serial]
#
# [bus.poll]
# dip_addresses = [0]
# interval_ms = 2000
# fault_every = 10
#
# --- A second bus, the RS-232 Voltronic/PI30 inverter leg (decision 0001) — runs a second,
# concurrent TMS identity (CONV role) alongside the battery's STOR role. See §2.1 for wiring a
# second physical UART.
# [[bus]]
# name = "inverter"
# protocol = "voltronic"
# [bus.serial]
# kind = "rs232"
# port = "/dev/ttyAMA2"
# baud = 2400                       # Voltronic/PI30 fixed baud
# [bus.poll]
# dip_addresses = [0]               # unused by the voltronic poll loop; kept for schema uniformity
# interval_ms = 2000
# fault_every = 10                  # QPIWS fault-word poll cadence

# --- Raw pre-parse wire capture-to-disk (decision 0001's field-diagnostics tool) — hex-encodes
# every raw request/response exchange (Seplos and/or Voltronic) to a rotating JSONL log, distinct
# from [disk]'s already-parsed/rendered TMS telemetry tail. Opt-in by presence; no default `dir`
# (an explicit field-diagnostics choice, not a Day-1-enabled feature). See
# the `persist` module in crates/lib-io.
# [capture]
# dir = "/var/lib/sentient/capture"
# max_file_bytes = 10485760         # 10 MiB per file
# max_total_bytes = 104857600       # 100 MiB total footprint across active + rotated segments
# queue_depth = 256                 # channel depth between the capturing transport and the writer task
```

`read_timeout_ms` is the fix for a silent or half-responding bus: the transport bounds each read
and returns a timeout instead of blocking forever.

**`[capture]` wiring status:** the config surface, the `RawCaptureRecord` type, and
`run_capture_writer` (rotation/disk-full/fsync, reused from the `[disk]` writer) are landed and
tested (see `lib-io`'s `persist` module in the [API reference](../reference/README.md)). The `SerialTransport` decorator that would actually
feed captured bytes from a live poll loop, and the `runtime.rs` spawn wiring that would connect it,
are a separate, tracked follow-on — presence of `[capture]` in the config is what will enable the
tap once that wiring lands.

**The top-level `direction` key must precede the first `[section]` header** (`[device]` above) —
TOML attaches a bare `key = value` to whichever table most recently opened, so it cannot be moved
lower in the file without also moving it into whatever table precedes it there.

**Broker bind is loopback by design.** The embedded broker is unauthenticated plaintext; it must
stay reachable only by the local mosquitto bridge. Widen `[broker] host` only deliberately and only
behind a firewall — see `security.md`.

---

## 5. mosquitto relay + the cloud-relay bridge (figment config, secret-only creds)

Install mosquitto and deploy `distro/deploy/mosquitto.conf`. It has one **static** bridge (reads the
device broker on `127.0.0.1:1883`, `topic tms/# in 1`) and an `include_dir` pointing at a directory
that holds the **rendered** cloud-relay bridge — mosquitto 2.x cannot expand `${...}` env vars in
its own config, so nothing in this repo ever ships a static file with placeholders for mosquitto to
fill in itself. Instead `bin-rpi` is the single source of truth:

- **Non-secret bridge settings** (`host`, `port`, `username`, `ca_file`, optional
  `remote_clientid`/`topic`/`qos`) live in the figment `[cloud]` section of
  `/etc/sentient/sentient.toml` (or `SENTIENT_CLOUD__*` env overrides) — see
  `distro/deploy/config.example.toml` for a filled-in example. A device with no `[cloud]` section at all
  is a valid, fully-supported local-only device: it still boots and serves the local broker.
- **The bridge password is never a config field.** It comes only from the `CLOUD_MQTT_PASS`
  environment variable, sourced from the secret file `/etc/sentient/cloud.env` (see
  `distro/deploy/cloud.env.example`) — never TOML, never git, never logged (`Config`/`CloudConfig`'s
  `Debug` output cannot contain it; there is no field to print).
- **The render step:**
  ```sh
  bin-rpi --emit-mosquitto-bridge /etc/sentient/mosquitto.d/cloud-relay.conf
  ```
  reads the figment `[cloud]` config plus `CLOUD_MQTT_PASS` and writes the real `connection
  cloud-relay` block (0600 perms) to that path. **Fails closed:** if `[cloud]` is present but
  `CLOUD_MQTT_PASS` (or `host`/`username`/`ca_file`) is missing/empty, it errors out rather than
  emit a bridge with an empty credential. If `[cloud]` is absent entirely, it emits nothing (and
  removes any stale rendered file) — local-only device, not an error.

Install:

```sh
sudo mkdir -p /etc/sentient/mosquitto.d
sudo chown mosquitto:mosquitto /etc/sentient/mosquitto.d && sudo chmod 0750 /etc/sentient/mosquitto.d
sudo install -D -m0644 distro/deploy/mosquitto.conf          /etc/mosquitto/conf.d/sentient.conf
sudo install -D -m0644 distro/deploy/mosquitto-override.conf /etc/systemd/system/mosquitto.service.d/override.conf
sudo install -D -m0600 cloud-ca.pem                    /etc/sentient/cloud-ca.pem
sudo install -o mosquitto -g mosquitto -m0600 cloud.env /etc/sentient/cloud.env   # from distro/deploy/cloud.env.example
sudo systemctl daemon-reload && sudo systemctl restart mosquitto
```

`distro/deploy/mosquitto-override.conf` is a systemd drop-in for the *package* `mosquitto.service`: it
sources `/etc/sentient/cloud.env` (`EnvironmentFile=-...`, optional — a local-only device need not
have this file) and runs the render step as `ExecStartPre`, so the bridge is always freshly
rendered from current config + secret before mosquitto starts.

TLS to the cloud is enforced (`bridge_insecure false`); certs/keys and the rendered bridge file are
root/mosquitto-owned `0600`. See `security.md` for holding the bridge client key in a TPM.

---

## 6. systemd service

On the shipped Yocto image, the hardened unit (packaged from `distro/deploy/bin-rpi.service` as
`sentient-agent.service`) is **already installed and enabled by the app sysext extension** — it
merges into `/usr/lib/systemd/system/` alongside `bin-rpi` when `systemd-sysext` merges the `.raw`
(see §3b), so there is no manual `install`/`enable` step on a normal deploy; the unit is live as
soon as the extension is merged and `systemd-sysext refresh` (or a reboot) runs. Tail its logs the
same way as any unit:

```sh
sudo systemctl status sentient-agent
sudo journalctl -u sentient-agent -f
```

The commands below (manually installing the unit file) are for a **non-Yocto / bare-Linux
bring-up** — e.g. running `bin-rpi` on a generic Debian/Raspberry Pi OS host outside the sysext
model, for development or bring-up before the image path is available:

```sh
sudo install -m0644 distro/deploy/bin-rpi.service /etc/systemd/system/bin-rpi.service
sudo systemctl daemon-reload
sudo systemctl enable --now bin-rpi
sudo journalctl -u bin-rpi -f
```

Key behaviour (same unit content either way):

- **`Type=notify` + `WatchdogSec=30`.** The binary sends `READY=1` after startup and `WATCHDOG=1`
  every **healthy** cycle. If the loop wedges or stays unhealthy (see below) it stops petting the
  watchdog and systemd restarts it (`Restart=on-failure`).
- **`ExecStartPre=verify-binary.sh`** verifies the binary's X.509 CMS signature (`openssl cms
  -verify` against `/etc/sentient/ca.crt`) and refuses to start on a mismatch (integrity gate — see
  `security.md`).
- **Least privilege:** runs as the `sentient` user with only `dialout` + a `DeviceAllow` for the
  serial port, under a full sandbox (`ProtectSystem=strict`, `NoNewPrivileges`,
  `MemoryDenyWriteExecute`, empty `CapabilityBoundingSet`, `SystemCallFilter=@system-service`).

If a build lacks the sd-notify integration, set `Type=simple` and delete `WatchdogSec=` (the unit
notes this).

---

## 7. Health log line

Every 30 cycles the loop logs a health summary — the field to watch in `journalctl`:

```
health: cycles=120 tlm ok/err=118/2 flt ok/err=11/1 pub ok/err=129/0 fail_streak=0
```

| Field | Meaning |
|---|---|
| `cycles` | Completed poll cycles. |
| `tlm ok/err` | Telemetry (0x42) exchanges decoded vs failed (transport or parse). |
| `flt ok/err` | Fault (0x44) exchanges decoded vs failed. |
| `pub ok/err` | TMS payloads the broker accepted vs rejected. |
| `fail_streak` | Consecutive cycles with zero successful exchanges. |

`fail_streak` is the health signal: at ≥3 the loop is reported **unhealthy** (watchdog feed stops)
and it attempts a serial reconnect after each backoff. A healthy device holds `fail_streak=0` and a
climbing `pub ok`.

---

## 8. Failure-mode runbook

| Symptom | Cause | System behaviour | Operator action |
|---|---|---|---|
| `telemetry dip=N skipped` occasional | transient bus glitch / one bad frame | logged + skipped, loop continues | none if intermittent |
| `dead cycle … backing off M ms` | whole cycle failed | exponential backoff 1 s → 30 s, retries | check wiring/power if it persists |
| repeated dead cycles, then `serial reconnect … ok` | device re-enumerated (USB unplug/replug) | in-loop reconnect after 3 failed cycles | none — self-recovered |
| `serial open …: No such file or directory` | wrong `port` or adapter absent | `OpenError` detail in journal; service may exit → systemd restart | fix `[serial] port`, replug adapter |
| only timeouts, never a frame | A/B swapped, wrong baud, wrong DIP | every read times out → dead cycles | swap A/B, confirm 19200 8N1, check DIP |
| service restarts in a loop | binary unhealthy or watchdog starved | `WatchdogSec` restart; signature failure blocks start | `journalctl -u bin-rpi`; re-verify signature |
| no data at the cloud | mosquitto bridge down / bad creds | device broker still serves locally | check `mosquitto` logs; re-run `bin-rpi --emit-mosquitto-bridge` by hand and read its stderr |
| mosquitto refuses to (re)start | `[cloud]` configured but `CLOUD_MQTT_PASS`/host/username/ca_file missing | render step exits non-zero (`ExecStartPre` fails) — fail-closed by design | fix `/etc/sentient/cloud.env` or `[cloud]` in the TOML, `systemctl restart mosquitto` |

Verify the local path independent of the cloud:

```sh
mosquitto_sub -h 127.0.0.1 -p 1883 -t 'tms/#' -v      # should print TMS JSON each cycle
```

---

## 9. Day-1 Sentient onboarding (private-cloud endpoint)

This is the exact, complete list of what #18 must deliver, and exactly where each value goes.
Nothing else about the device changes — no code, no rebuild — once these land.

**#18 must provide:**

| Value | Goes in | Secret? |
|---|---|---|
| Cloud MQTT host | `[cloud] host` in `sentient.toml` | No |
| Cloud MQTT port (TLS, usually 8883) | `[cloud] port` in `sentient.toml` (defaults to 8883) | No |
| Bridge username | `[cloud] username` in `sentient.toml` | No |
| Bridge password | `CLOUD_MQTT_PASS` in `/etc/sentient/cloud.env` | **Yes — secret file only** |
| CA certificate (to verify the cloud endpoint's TLS cert) | file dropped at the path named by `[cloud] ca_file` (e.g. `/etc/sentient/cloud-ca.pem`, 0600) | No (but keep it root/mosquitto-owned) |

**Runbook:**

1. Fill in `[cloud]` in `/etc/sentient/sentient.toml` (host/port/username/ca_file; see
   `distro/deploy/config.example.toml`) and drop the CA cert at the path `ca_file` names, root-owned 0600.
2. Put the password in `/etc/sentient/cloud.env` as `CLOUD_MQTT_PASS=...` (see
   `distro/deploy/cloud.env.example`), owned `mosquitto:mosquitto`, mode `0600`. Never in git, never in
   TOML.
3. `sudo systemctl restart mosquitto` — the `ExecStartPre` (§5) re-renders
   `/etc/sentient/mosquitto.d/cloud-relay.conf` from the config + secret and mosquitto picks it up
   via `include_dir`. If either value is missing, the restart fails closed (mosquitto does not
   start with an empty-credential bridge) — check `journalctl -u mosquitto` for the render error.
4. Confirm the `cloud-relay` bridge connects (mosquitto log: `Connecting bridge cloud-relay`, then
   no reconnect loop).
5. Validate publish reachability with the env-gated integration test:
   ```sh
   SENTIENT_TEST_ENDPOINT=host:port cargo test -p lib-mqtt --test broker_roundtrip -- --ignored
   ```
   This publishes a golden `DcMeasurementUpdate` to the configured endpoint and asserts the broker
   acknowledged it — the switch to flip once creds land.

**What still needs a real Sentient value to fully validate today:** everything in the table above.
The `[cloud]` schema, the render step, and the fail-closed behavior are all covered by
`cargo test -p bin-rpi` against fixture values — but an end-to-end bridge connect against the
*real* private-cloud endpoint can only be exercised once #18 hands over an actual host/port/
username/password/CA cert.

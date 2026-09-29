# Yocto QEMU functional e2e (Task #156 / Story #159)

**Verdict-style summary (mirrors the retired MCU harness README's spike-report format): this e2e is
possible for the FIRST TIME because the Yocto Linux guest has a real, emulatable network stack —
the old Zephyr-on-Pi plan had no emulated NIC for the Pi (see `CLAUDE.md`'s former "RPi networking
is BLOCKED" doctrine: no Ethernet L2 driver for the Pi 5 in Zephyr, no in-tree Broadcom GENET, no
RP1 MAC, no devicetree node). Running Linux on the Pi dissolves that blocker outright: the mainline
kernel's `bcmgenet`/RP1 drivers own L2, and `runqemu`'s `qemuarm64` emulation-proxy machine gets a
real slirp user-net NIC + a real UART for free.** This directory is the harness that proves it,
greenfield-authored per the plan (`/home/jstandish/.claude/plans/glittery-frolicking-peach.md`,
Sub-task 8).

## What this proves

Boot the actual shipping artifact — `sentient-image` (the same recipe that ships on
`raspberrypi4-64`/`raspberrypi5`, see `distro/README.md`) — under `runqemu`, headless, with no
hardware. Feed it simulated Seplos BMS RS-485 frames on a serial port. Assert its embedded MQTT
broker (`rumqttd`, inside `bin-rpi`) publishes the three TMS payload kinds an external, stock paho
MQTT client can subscribe to and validate:

- `DcMeasurementUpdate` (`tms/METEOR-001/dc_measurement_update`)
- `PowerHardwareUpdate` (`tms/METEOR-001/power_hardware_update`)
- `ActiveDiagnosticState` (`tms/METEOR-001/active_diagnostic_state`)

This is a **functional** e2e (real Seplos frames -> real parser -> real mapper -> real broker ->
real external MQTT client), not a boot-smoke — the Linux/Yocto-pivot equivalent of what the
(now-retired) MCU `qemu-e2e.sh` harness proved for the Zephyr firmware on `qemu_cortex_m3`.

## Files (this directory — sole ownership per Task #156)

| File | Role |
|---|---|
| `yocto-e2e.sh` | The standalone dev/local harness: warm container + persistent build cache, `kas build --target sentient-image`, `runqemu`-driven boot, SeplosSim-over-serial, paho-over-slirp-hostfwd assertion. |
| `lib/tms_mqtt_check.py` | The external MQTT client + assertion used by `yocto-e2e.sh`. Imports `lib/tms_validate.py`'s validators **verbatim** (plain sibling import) — the TMS payload schema is defined ONCE, reused here, never re-derived. |
| `e2e_telemetry.robot` | **The #156 Robot Framework suite** uat.yml's "Run Robot e2e" step runs (`robot ... distro/qemu/`). Boots `sentient-image` fresh (its own `runqemu`, independent of uat.yml's preflight boot, which is already dead by the time this step runs), drives `seplos-tcp-sim` with the deterministic `BatteryState::sample_14_cell()` fixture, and asserts the three TMS payload kinds by SCHEMA + VALUE (not just "a message arrived") via `lib/tms_validate.py`. |
| `e2e_common.resource` | Suite Setup/Teardown + shared variables for `e2e_telemetry.robot`. |
| `lib/YoctoQemuKeywords.py` | Robot library that boots `sentient-image` under `runqemu` (NOT a bare `qemu-system-*` call — see its own module docstring) and captures the console; the Yocto/`runqemu` analogue of the retired MCU harness's `QemuKeywords.py`. |
| `lib/MqttKeywords.py` | Robot library: a stock paho-mqtt v2 external subscriber (connect, subscribe, wait/collect per topic, arrival timestamps, traffic log). Platform-agnostic; relocated here from the retired MCU harness, unchanged. |
| `lib/SeplosSimKeywords.py` | Robot library: `seplos-tcp-sim` subprocess lifecycle + its stdin control protocol (`behavior`/`state`/`faults`). Platform-agnostic; relocated here from the retired MCU harness, unchanged. |
| `lib/tms_validate.py` | Pure, I/O-free TMS payload validators written against `tms-compliance.md`. Loaded as a Robot `Library` by `e2e_common.resource` **and** imported as a module by `lib/tms_mqtt_check.py` — one schema source of truth. Relocated here from the retired MCU harness, unchanged. |
| `README.md` | This file. |

> **Provenance note.** `MqttKeywords.py`, `SeplosSimKeywords.py` and `tms_validate.py` originally
> lived in the MCU (Zephyr) QEMU harness and were imported cross-tree by this suite so the TMS
> schema had exactly one definition. That harness was retired with the Embassy pivot
> (`docs/decisions/0010-mcu-runtime-embassy.md`) and its tree deleted, which broke every `Library`
> import here (issue #380). The three libraries — all platform-agnostic — were relocated into
> `distro/qemu/lib/` verbatim; this suite is now their sole owner and consumer. `QemuKeywords.py`
> was deliberately NOT relocated: it launched `qemu_cortex_m3` for the MCU firmware and has no
> consumer here (`YoctoQemuKeywords.py` is this suite's launcher).

### KNOWN GAPS in uat.yml's wiring for `e2e_telemetry.robot` — flagged, not silently worked around

Discovered while authoring the suite (Task #156); **not fixed here** since fixing them means editing
`.github/workflows/uat.yml` / `distro/meta-sentient/**`, outside this suite's file ownership. The
suite documents each assumption inline (see `lib/YoctoQemuKeywords.py`'s `start_yocto_qemu`
docstring) and will fail loudly, with a diagnosable message, if these aren't addressed — not pass
silently on a broken wire-up.

1. **No QEMU instance survives from uat.yml's preflight into the Robot step.** The "Preflight" step
   boots `sentient-image` under `runqemu`, checks the console for the sysext-merge + agent-started
   signals, then explicitly kills `$QPID` (+ `pkill -f qemu-system`). "Build Seplos sim" and "Run
   Robot e2e" are later, independent shell steps in the SAME job with no QEMU alive in between.
   `e2e_telemetry.robot` therefore boots its OWN fresh `runqemu` instance (via
   `YoctoQemuKeywords.start_yocto_qemu`, using the `IMAGE_DIR`/`MACHINE` variables uat.yml already
   passes via `robot --variable`) — this is by design, not an oversight, but it means the suite pays
   a second full boot.
2. **The embedded broker binds to `127.0.0.1` only, and uat.yml never widens it.** The shipped
   `distro/meta-sentient/recipes-sentient/sentient-agent/files/sentient.toml` (baked into the app
   sysext, unmodified by uat.yml) sets `[broker] host = "127.0.0.1"`
   (`crates/bin/rpi/src/config/impl_broker_toml.rs`'s `Default`). A `runqemu ... slirp`
   `hostfwd=tcp::PORT-:1883` forwards a HOST port to the GUEST's own IP:1883 — it **cannot** reach a
   service bound only to the guest's loopback. `yocto-e2e.sh`'s standalone harness solves this by
   rendering an e2e-only `sentient.toml` (`[broker] host = "0.0.0.0"`) and `wic cp`-ing it into the
   image BEFORE boot (see that script's `inject_e2e_config`). **uat.yml's pipeline has no equivalent
   step** — nothing widens the broker bind for the merged-sysext boot the Robot suite drives. Until
   this is wired (either a `wic cp`/debugfs-write step in uat.yml before the Robot step, or an
   env-var/config override baked into the sysext for CI runs), `MqttKeywords.Connect External
   Subscriber` in `e2e_common.resource`'s `Standard Yocto E2e Setup` will time out.
3. **The Seplos serial link's guest device path is unconfirmed, AND the systemd unit's
   `DeviceAllow` doesn't match the shipped config.** The shipped `sentient.toml` sets `[serial] port
   = "/dev/ttyAMA0"`, but `sentient-agent.service` (`distro/meta-sentient/recipes-sentient/
   sentient-agent/files/sentient-agent.service`) sets `DeviceAllow=/dev/ttyUSB0 rw` under
   `DevicePolicy=closed` — its OWN inline TODO already flags this mismatch. Independently, whether
   `qemuarm64` exposes a second UART at all (this suite's `-chardev socket,...` /
   `-serial chardev:seplossim` qemuparams target) that maps to `/dev/ttyAMA0` in the guest is the
   SAME open question `yocto-e2e.sh`'s README already flags (see "TODO(verify-at-build)" item 1
   below). If either is wrong, `HostSerial::open` fails at boot; per
   `impl_host_platform.rs::HostPlatform::build`'s doc comment this is NON-FATAL (the agent starts
   DEGRADED, broker up, serial `Disconnected`) — so the suite's boot/agent-started waits will still
   pass, but NO Seplos frames are ever read and every `Wait For Message On Topic` call will time out
   instead. A DEGRADED-start console log line is the tell if this is what's happening.

## Reused, not reinvented (what came from elsewhere, and why it's safe to reuse)

- **`crates/test-kit`'s `SeplosSim`** (`crates/test-kit/src/seplos/`) via the `seplos-tcp-sim` binary
  (`crates/test-kit/src/bin/seplos_tcp_sim/`, built with `cargo build -p test-kit --features
  tcp-sim --bin seplos-tcp-sim`). This is the SAME simulator the retired MCU harness's
  `qemu-e2e.sh` used — it decode-reuses `lib-parse`'s Seplos frame/checksum encoder so the sim and
  the real decoder can never silently drift apart. `seplos-tcp-sim --connect HOST:PORT` dials in as
  a TCP client; a stdin control protocol (`behavior <name>` / `state <name>` / `faults <list>`) can
  steer it mid-run, exactly as `lib/SeplosSimKeywords.py` drives it for this Robot suite.
- **`lib/tms_validate.py`** — pure, I/O-free TMS payload validators
  (`validate_dc_measurement_update`, `validate_power_hardware_update`,
  `validate_active_diagnostic_state`, `validate_topic_slug`, plus the sign-inversion/NaN-policy
  checks) written against `tms-compliance.md`. `lib/tms_mqtt_check.py` imports this file directly
  (plain sibling `import tms_validate`) rather than copying its rules — a schema change only needs
  updating in one place. `e2e_common.resource` imports it a SECOND way — as a Robot `Library`
  (`${CURDIR}/lib/tms_validate.py`) — so `e2e_telemetry.robot`'s test cases call
  `Validate Dc Measurement Update` / `Validate Power Hardware Update` /
  `Validate Active Diagnostic State` / `Check Sign Inversion` / `Check Nan Policy` as ordinary
  Robot keywords, matching the call convention the retired MCU compliance suite established.
  (Before issue #380 this file lived in the MCU harness and was reached cross-tree; see the
  provenance note above.)
- **`lib/MqttKeywords.py`** and **`lib/SeplosSimKeywords.py`** — imported verbatim (no fork) by
  `e2e_common.resource`; unlike the MCU harness's `QemuKeywords.py` (Zephyr/`qemu_cortex_m3`-
  specific launch, deliberately not relocated), these two libraries are platform-agnostic (a paho
  subscriber, and a `seplos-tcp-sim` subprocess controller) and apply unchanged to the Yocto guest.
- **The retired MCU harness's `mqtt_check.py`** — the shape (paho client, `on_connect` subscribes to `tms/#`
  + `monitor/#`, bounded wait loop, PASS/FAIL summary, matching exit code) is the template
  `lib/tms_mqtt_check.py` follows. It is a new file, not an edit of that one, because the assertion
  itself is stricter here (validates all three payload kinds by CONTENT against `tms_validate.py`,
  not just "a message arrived under `tms/#`") and the connection target is fundamentally different
  (slirp hostfwd `127.0.0.1:<port>` vs. the Zephyr harness's TAP peer `192.0.2.1:1883`).
- **The retired MCU harness's `qemu-e2e.sh`** — the STRUCTURE (warm long-lived container + host-persistent
  build-cache bind mount + a `sync` step that tars the repo into the cache instead of bind-mounting
  it directly + a bounded early-connect retry loop for the sim + a `run [N]` CLI convention) is what
  `yocto-e2e.sh` is modeled on. It is genuinely a **different** script, not a copy: there is no
  `west`/Zephyr/CMake build here at all (Yocto's build is `kas build ... --target sentient-image`,
  BitBake under the hood), and the run step is `runqemu` (which locates the MACHINE's kernel/rootfs
  artifacts a `kas build` just produced) rather than a bare `qemu-system-*` invocation. Container
  name (`yocto-e2e` vs. `qemu-e2e`), cache dir (`/tmp/yocto-build-cache` vs.
  `/tmp/qemu-build-cache`), and network wiring (slirp + TCP-loopback chardevs vs. a `zethc` TAP) are
  all kept distinct on purpose so the two harnesses never collide if run concurrently on the same
  host/CI node.

## Network + serial wiring (verified facts + citations, per the plan)

- **Machine**: `qemuarm64` (aarch64 — the faithful Pi 4/5 64-bit stand-in). **ARM only — x86 is not
  a supported platform.** Confirmed: bare `raspberrypi5` MACHINE images do **not** boot under
  `runqemu` — only the `qemuarm64` emulation-proxy machine does. Same `sentient-image` recipe, same
  prebuilt musl-static `bin-rpi`.
- **Reaching the guest MQTT broker (1883) from the host**: `runqemu`'s slirp user-net port-forward —
  `-netdev user,id=net0,hostfwd=tcp::<HOST_PORT>-:1883`, passed via `runqemu <machine> ...
  qemuparams="..."`. An external paho MQTT client then connects to `127.0.0.1:<HOST_PORT>`.
  `yocto-e2e.sh` uses host port **18830** (not 1883) specifically so a locally-running mosquitto (or
  any other broker already bound to 1883 on the dev/CI host) is never shadowed.
- **Feeding the guest serial from the host**: a host-side QEMU `-chardev socket,...,server=on,
  wait=off` attached to a guest UART via `-serial chardev:<id>`, passed through `runqemu`'s
  `qemuparams=`. `seplos-tcp-sim` then dials that socket as a TCP **client**
  (`--connect 127.0.0.1:<port>`) — the same "QEMU is the server, the sim dials in" pattern
  `qemu-e2e.sh` already proves works reliably (early-connect + bounded retry beats the firmware's
  exponential poll-cycle backoff).
- **Headless CI**: `runqemu <machine> nographic slirp serial` boots with no display/hardware.
  Yocto's own `testimage`/`oeqa` runtime-test framework is the idiomatic driver for this kind of
  assertion (it's how Yocto's own release QA works) — this harness deliberately picks the simpler
  **plain scripted `runqemu` + log-grep/expect-style wait** approach instead, because it's faster to
  author and verify without a live Yocto environment; `testimage`/`oeqa` remains the documented
  idiomatic alternative for a future hardening pass.
- **Assertion**: the external paho client (`lib/tms_mqtt_check.py`) subscribes to `tms/#` +
  `monitor/#` and requires ALL THREE TMS payload slugs to appear AND schema-validate before passing.

## TODO(verify-at-build) — everything flagged, nothing fabricated

Per this task's explicit constraint (no local Yocto/runqemu/QEMU image to validate against), every
non-load-bearing-but-unconfirmed detail is flagged inline in `yocto-e2e.sh` and summarized here:

1. **Second guest UART**: does `qemuarm64` expose a second UART (`/dev/ttyS1`, this harness's
   working assumption) out of the box, or does it need a devicetree/kernel-cmdline tweak? Check the
   pinned poky's `meta/conf/machine/qemuarm64.conf` + its `qemuboot.conf` at build time. If only one
   UART exists, the fallback (documented inline in `yocto-e2e.sh`) is to multiplex: move the boot
   console to `mon:stdio` and dedicate the one real UART to the Seplos link.
2. **`runqemu` keyword vs. explicit `qemuparams=` interaction**: passing an explicit
   `-netdev`/`-serial` alongside the `slirp`/`serial` convenience keywords needs confirming those
   don't double up or conflict — if they do, drop the bare keywords and pass everything through
   `qemuparams=` explicitly.
3. **Injecting the e2e-only `sentient.toml`** (`[broker] host = "0.0.0.0"`, `[serial] port =
   "/dev/ttyS1"`) into a **read-only-rootfs** image at build time via Yocto's `wic cp` tool — needs
   confirming `wic cp` works against this recipe's actual wic-plugin/partition layout in
   `sentient-yocto-builder`. A documented fallback (an e2e-only BitBake mechanism) exists but isn't
   implemented here since it would mean editing `distro/meta-sentient/**`, outside this task's file
   ownership.
4. **Agent-ready banner text**: `yocto-e2e.sh` polls the console log for `"sentient agent ready"` or
   a login prompt as the boot-complete signal — confirm the exact banner text once the
   `ExecStartPost`/`notify_ready()` wiring (CLAUDE.md Phase 1, owned by another workstream) lands.
5. **`poky/oe-init-build-env` path**: assumes kas's default repo layout puts the `poky` checkout as
   a sibling of the `build/` directory it also creates — this is kas's documented default, but has
   not been observed against a real `kas build` run in this environment.
6. **[broker] host = "0.0.0.0" is an e2e-only relaxation**, not a hardening regression: widening
   beyond `127.0.0.1` ships today with `auth: None` (see
   `crates/bin/rpi/src/config/impl_config.rs::broker_config`'s own doc comment — no CONNECT
   authorizer backend exists yet in `lib-security`). Safe here because the QEMU slirp segment is
   reachable only from the process that started `runqemu`; **never** copy this config into a real
   deployment's `sentient.toml`.

## Usage

```bash
./distro/qemu/yocto-e2e.sh up                       # start (or reuse) the warm container
./distro/qemu/yocto-e2e.sh build qemuarm64          # kas build sentient-image only
./distro/qemu/yocto-e2e.sh run 1 qemuarm64          # build + boot + simulate + assert, once
./distro/qemu/yocto-e2e.sh run 3 qemuarm64          # same, 3 times
./distro/qemu/yocto-e2e.sh down                     # stop + remove the warm container
```

`just yocto-e2e-run [N]` (added by the CI workstream) wires this in for local dev parity with CI —
see the root `justfile`'s `yocto-e2e-run` recipe.

## Where this runs in CI

`.github/workflows/_yocto.yml` (this task) is a reusable workflow that runs this harness on the ARC
runners inside `sentient-yocto-builder`, for `qemuarm64` (ARM only), and fails loudly on a
missing/incorrect TMS payload. It is a **release/stage-3 lane** (like `_emulation.yml`'s Renode
boot-smoke), not the fast PR gate — `release.yml` (owned by another workstream) is expected to call
it; see that file's own placeholder comment after its `yocto` job. No brute-force/native stress
testing is included or intended here — this lane asserts one clean functional round trip per
emulation machine.

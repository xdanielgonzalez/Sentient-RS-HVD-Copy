# Parity Gap Analysis — sentient-rs vs. meteor-adapter

**Date:** 2026-07-16 (latest refresh: **2026-07-27, later same day** — see bottom-most Addendum; not re-scored, see its own methodology note)
**Baseline (previous implementation):** `high-velocity-decisions/meteor-adapter` (Python prototype)
**Current:** `high-velocity-decisions/sentient-rs` (production Rust adapter)
**Previous run:** 2026-07-15/16, base commit `771fd20`. This run is a re-verification triggered by the "gap closure" merge **`11b165f`**, landed on `main` since the prior report.
**Method:** 8 analyst agents (one per capability dimension — **Security & deployment is now its own tracked dimension**, split out of the prior report's aggregate "Where sentient-rs is ahead" bullets) → adversarial verifier per dimension re-checking every claim tied to the merge → completeness-critic sweep. All 8 dimension verifiers completed this run (last time ops-tooling's verifier failed mid-run; that gap is closed). **47 capabilities were scored this pass** — a targeted re-verification of items the merge touched or materially bears on, not a full re-score of all 95 baseline capabilities from the prior report. Items untouched by the merge (e.g. most of the previously-scored "superior" core-path-rigor and security bullets) retain their 2026-07-15 verdict and were spot-checked for merge impact (`git show 11b165f --stat`) rather than fully re-derived; four Security & deployment items fall in this "carried forward, not independently re-examined" category — flagged inline in that section.

---

## Changelog since last run

**Gaps CLOSED** (previously missing/partial → now parity/superior):

- **Multi-port serial (InterfaceManager-equivalent)** — missing/high → **parity**. `bin-rpi/src/config/types/{config.rs,bus_config.rs}`'s `[[bus]]` array-of-tables + `Config::buses()` + `runtime.rs::supervise` now fan out one `HostPlatform`/task per bus into a `JoinSet` off one shared broker. (Serial transport)
- **Heartbeat topic** — missing/medium → **superior**. `bin-rpi/src/tasks/heartbeat.rs::run_heartbeat` publishes a real 1 s heartbeat over live MQTT; baseline never wires one at all. (TMS data models)
- **Request/Reply control path** — missing/high → **superior**. `bin-rpi/src/tasks/reply_listener.rs` answers within the TMS 0.1 s budget, proven by `test-e2e/tests/request_reply_e2e.rs`; baseline never wires this either. (TMS data models)
- **StorageUpdate payload type** — missing/medium → **superior**. Wired every cycle via `transform.rs::to_storage_update` → `publish_storage_update_tapped`, proven in `multi_port_e2e.rs`. (TMS data models)
- **SIGTERM handling + graceful shutdown drain** — partial/medium → **parity**. Real `SIGTERM` handler + bounded 5 s drain, proven by `test-e2e/tests/sigterm_drain_e2e.rs` (real binary, real signal, real pty). (App runtime & config)
- **`deny_unknown_fields` on config** — partial/low → **parity**. All 11 config-section structs now reject unknown keys; env-var override path reconciled. (App runtime & config)
- **Disk / JSON-lines telemetry logging (Scope A)** — missing/high → **superior**. `lib-persist` (rotation, ENOSPC recovery, drain-on-shutdown) is real and wired; exceeds baseline's unbounded, unrecoverable `DiskPublisher`. (Publish targets / Ops & docs)
- **Publisher fan-out with per-target isolation** — missing/high → **superior** *(mechanism itself; see Scope D note below on one consumer)*. Non-blocking per-tap `try_send` via `SinkSet`/`HostTap`, arguably better than Python's blocking `asyncio.gather`. (Publish targets)
- **Waveshare HAT setup docs** — partial/medium → **parity**. `docs/deployment.md` §2.1 now has the exact `config.txt` overlay lines and device-node table. (Ops & docs)
- **Capture-analyzer / field-diagnostic tooling** — missing/low (unverified) → **parity**. New `bin-rpi --scan-bus` DIP sweep, functionally equivalent to (and more correct than) the baseline's scanner. (Ops & docs)

**Gaps materially NARROWED but still open:**

- **DeviceInfo family** — missing/high → **partial/medium**. Now genuinely constructed and published every 60 s (`build_device_info`), but the schema itself is far narrower than baseline's (no `DeviceRole`, `ControlHardwareInfo`, `PowerHardwareInfo`).
- **Meshtastic mesh-radio forwarding** — missing/high → **partial/medium**. A real `lib-mesh` crate now exists and is wired, but the protobuf framing is self-flagged PROVISIONAL/unverified against real hardware, and fault forwarding is missing entirely (a genuine regression vs. baseline, which does forward faults over mesh). *(**Update 2026-07-27**: the fault-forwarding regression is now closed — see the addendum below. `HostTap::Mesh` forwards `TapEvent::Fault` immediately using the last cached reading for that DIP, and a separate, more serious defect was caught in the process: `MeshPacket.to` was never encoded, so frames likely weren't delivered at all before this fix, not merely unverified. Still open: inverter fault/reading forwarding over mesh, and real-hardware HIL validation of the codec.)*

**Gaps OPENED / newly found:**

- **Contract Scope D overclaim** *(new, high severity)* — the gap-closure merge's commit message/CHANGELOG claim Scope D closure. It does not hold: `impl-app/src/host/impl_host_tap.rs`'s `HostTap::Greengrass` dispatch arm is a no-op trace log — it never calls `tap.offer()` — so `legacy/#` never receives a publish in the shipped binary, despite a complete renderer (`lib-mapping/src/legacy/`) and bridge type existing. `docs/audit-2026-07.md`'s NOT MET verdict for Scope D remains correct.
- **AWS-Greengrass legacy schema transform, field-level divergence** *(new, high severity)* — even bypassing the dispatch stub, the new legacy-schema mapper diverges from the real `example/aws` battery pipeline on topic shape, field names, array-vs-flat-key structure, and which fields are ever populated. A downstream consumer built against the real legacy schema would not parse this output.
- **Location service (`lib-locate`)** *(new to this report's gap table; previously only noted as absent under "Legacy AWS pipeline")* — a real 4-tier fallback chain (Manual/GpsNmea/ModemManager/IpGeo) now exists and is wired, materially closing what was a total gap. Corrected here from the raw analyst's "superior" call to **partial/medium**: the baseline actually runs a 6-tier chain (adds Cloud Override via Greengrass and Cell-Tower via Mozilla Location Service) that the Rust chain has no equivalent for.

**Contract scope status changes:**

- **Scope A:** NOT MET → **MET**, and arguably exceeded (rotation + disk-full recovery baseline never had). `docs/audit-2026-07.md` (dated 2026-07-15, base commit `771fd20`) is now **stale** on this row — it predates merge `11b165f` (2026-07-16) by one day.
- **Scope D:** unchanged — **still NOT MET**. New renderer/config/bridge code exists, but the runtime dispatch that would actually emit `legacy/#` is a stub, and the schema itself has confirmed field-level mismatches against the real legacy pipeline. The merge's own claim of closing Scope D does not survive verification.
- **Scopes B, C, E:** unchanged.

---

## Verdict

**The gap-closure merge (`11b165f`) closes real ground on the core telemetry path — multi-port fan-in, SIGTERM/drain, disk persistence, and three new TMS topics are now genuinely wired and, on several axes, superior to the baseline. It does not close what it claims to close on Scope D, and it opens one clear regression (Meshtastic no longer forwards faults, where the baseline did).**

Read together with the prior report, the picture is: the deliberate-descope items (AC measurement models, RS-232 inverter leg, Meshtastic — now partially built) are unchanged in kind, still decisions to be made explicitly rather than accidents. The "true gap" items from last week (Scope A, multi-port, fan-out) are substantially resolved. One new category has appeared this run: **claimed-vs-actual mismatch** — code that exists, is well-built, and is cited in a merge/CHANGELOG as closing a gap, but is not actually reachable at runtime (Greengrass dispatch) or not actually correct against the real reference schema (legacy field mapping). This is worth naming as its own risk category going forward: a merge asserting "Scope X closed" is not sufficient evidence that it is.

### How to read this

`KICKOFF.md` still explicitly declares the Python prototype *reference-only*. So "missing" never automatically means "must build"; each gap below is tagged as a **deliberate descope**, a **decision needed**, or a **true gap**. This run adds one more, informal tag used in a few places: **claimed-but-not-actually-wired** — a specific, verified instance of code existing without the runtime path that would make it observable in production.

### Status definitions

*(unchanged from the prior report)*

- **parity** — an equivalent capability exists in sentient-rs with the same observable behavior, verified in code (a claim in docs alone doesn't count). The mechanism may differ; the outcome doesn't.
- **superior** — parity, *plus* a concrete advantage on at least one objective axis, cited in the code of both repos.
- **partial** — the capability exists but with a material behavioral difference or a reduced surface.
- **missing** — no equivalent found anywhere in sentient-rs after repo-wide search.
- **n/a** — scored but excluded from gap-counting, with a stated reason per item.

---

## Scorecard by dimension

**Note on scope:** this run's scorecard covers **47 capabilities re-scored across 8 dimensions** (Security & deployment now broken out as its own row, versus 7 dimensions/95 items last time). It is a targeted re-verification of items the gap-closure merge touched or materially bears on — not a full re-run of all 95 baseline capabilities. Items not implicated by the merge keep their 2026-07-15 verdict and are not recounted here.

**Legend:** 🟩 Parity · 🟦 Superior · 🟧 Partial · 🟥 Missing · ⬜ N/A

| Dimension | Stack | Items |
|---|---|---|
| Serial transport | 🟩1 🟧1 🟥1 | 3 |
| Protocols (Seplos etc.) | 🟩1 🟦2 🟧1 🟥2 ⬜1 | 7 |
| TMS data models | 🟦3 🟧2 🟥2 | 7 |
| TMS transform | 🟦3 🟧3 ⬜1 | 7 |
| Publish targets | 🟦2 🟧2 🟥1 | 5 |
| App runtime & config | 🟩5 🟧1 | 6 |
| Ops, tests & docs | 🟩2 🟦1 🟧3 | 6 |
| Security & deployment | 🟦5 🟧1 | 6 |
| **Total (this run)** | 🟩9 🟦16 🟧14 🟥6 ⬜2 | 47 |

<details>
<summary>Exact counts as a table</summary>

| Dimension | Items | Parity | Superior | Partial | Missing | N/A |
|---|---|---|---|---|---|---|
| Serial transport | 3 | 1 | 0 | 1 | 1 | 0 |
| Protocols (Seplos etc.) | 7 | 1 | 2 | 1 | 2 | 1 |
| TMS data models | 7 | 0 | 3 | 2 | 2 | 0 |
| TMS transform | 7 | 0 | 3 | 3 | 0 | 1 |
| Publish targets | 5 | 0 | 2 | 2 | 1 | 0 |
| App runtime & config | 6 | 5 | 0 | 1 | 0 | 0 |
| Ops, tests & docs | 6 | 2 | 1 | 3 | 0 | 0 |
| Security & deployment | 6 | 0 | 5 | 1 | 0 | 0 |
| **Total (this run)** | **47** | **9** | **16** | **14** | **6** | **2** |

</details>

---

## Contract scope A–E

| Scope | Status | Evidence |
|---|---|---|
| A — local microSD data logging demo | **MET** *(changed this run)* | `crates/lib-persist` (JsonlWriter, rotation, ENOSPC recovery, drain-on-shutdown) is real and wired: `runtime.rs::build_taps` spawns `run_writer`, `HostTap::Disk` does a real bounded `try_send` of every publish. Proven end-to-end by `test-e2e/tests/sigterm_drain_e2e.rs` (spawns the actual compiled binary, sends real SIGTERM, checks JSONL integrity). Exceeds baseline's `disk.py`, which has no rotation, size cap, or disk-full handling at all. `docs/audit-2026-07.md` (2026-07-15, base `771fd20`) is now **stale** on this row. |
| B — private-cloud MQTT | **Met** (unchanged) | Embedded rumqttd broker + fail-closed mosquitto bridge, TLS 8883. Not touched by this merge. |
| C — TMS compliance | **Met — strongest** (unchanged, and stronger) | Typed schema + transform + cucumber scenarios; this merge added 4 more wired topics (Heartbeat, DeviceInfo, StorageUpdate, Reply) on top of the prior 3, all PROVISIONAL/not-IDL-verified per their own doc comments. |
| D — Greengrass backwards-compat | **NOT MET** *(re-confirmed, merge overclaims)* | New code exists — `lib-mapping/src/legacy/` (schema transform), `impl_greengrass_tap.rs` (renderer), a `[greengrass]` config section — but `impl-app/src/host/impl_host_tap.rs`'s `HostTap::Greengrass` dispatch arm never calls `tap.offer()`, so `legacy/#` is never actually published in the shipped binary. Separately, the legacy schema itself (checked field-by-field against `example/aws/.../battery_telemetry.py`/`config.yaml`) diverges on topic shape, field names, array-vs-flat structure, and always-disabled fields. The gap-closure merge's commit message/CHANGELOG claim to close Scope D; verification does not support that claim. Tracked: #52–#54 (need a new ticket specifically for the dead dispatch arm). |
| E — TPM security hardening (≤10h consult) | **Partial, honestly stubbed** (unchanged) | Not touched by this merge. |

---

## Verified gaps, by dimension

Only missing/partial items are tabulated below (parity/superior items are counted in the scorecard and discussed under "Where sentient-rs is ahead"). Ordered high → medium → low within each dimension.

### Serial transport

| Gap | Status | Severity | Detail |
|---|---|---|---|
| **RS-485 direction control mechanism** | partial | medium | The mechanism is now honestly and fully implemented at the `lib-io` layer — `host/types/direction_mode.rs`'s `RtsSoftware`/`KernelRs485`/`GpioDe` variants, real drain-before-deassert (`impl_rs485.rs`), real `TIOCSRS485`/`gpiocdev` calls in `open_with_direction`, and the previously-false doc comment in `host/mod.rs` is fixed. **But it is dead code at runtime**: `impl-app/src/host/impl_host_platform.rs`'s `build_port` and `reconnect` both call plain `HostSerial::open(...)`, never `open_with_direction` — the configured `DirectionMode` is stored on `HostPlatform` (`#[allow(dead_code)]`, doc comment literally says "STUB") and discarded. Every port in production still opens `RtsSoftware` only, regardless of TOML config. Grepped repo-wide: the only callers of `open_with_direction` outside `lib-io` itself are its own tests. *Decision/connection needed, not a design gap* — the last wiring step from config → `open_with_direction` was never taken. |
| **Passive stream reading + pluggable framing** | missing | medium | Unchanged. Seplos exchange is still fixed request/response, `\r`-delimited (`lib-parse/src/seplos/frame/mod.rs:24`). `lib-channel` (bounded, back-pressuring, not drop-on-full) remains depended on only by `test-kit`/`lib-net-embassy`/`lib-link` — confirmed via grep of every `Cargo.toml`, still unused on the host serial path. No framing-strategy abstraction landed in this merge despite other serial-layer work landing alongside it. |

### Protocols

| Gap | Status | Severity | Detail |
|---|---|---|---|
| **RS-232 inverter leg** | missing | high | Unchanged — zero `.rs` hits for voltronic/mpp-solar/qpigs/inverter; `lib-io/src/host/impl_rs232.rs` is direction-control no-op; `impl-app/src/lib.rs::run_cycle` hardcodes a single Seplos vendor. **New this run:** a formal, dated, *Locked* decision record now exists — `docs/decisions/0001-rs232-inverter-leg-in-scope.md` (2026-07-16; supersedes) — recording this as in-scope for this round rather than an implicit gap. The finding is unchanged in substance but is now a *recorded decision*, not a silent omission. Baseline's own `src/` has no inverter code either; it lives only in the reference-only `example/aws` legacy tree. |
| **RS-485 direction-control mechanism** *(shared with Serial transport)* | partial | medium | Same underlying finding as the serial-transport row above — the mechanism is real, the wiring into the shipping runtime is not. Listed here once for cross-reference; do not double-count. |
| **Raw serial-capture-to-disk mode** | missing | medium | *Severity reconciled with the Publish-targets row below (previously scored inconsistently — low here vs. medium there in the raw analyst output; unified at medium since the underlying gap is the same and disk persistence is now a real, shipped capability making this a more concrete near-term ask than it was last week).* No `RawBytes`/capture variant anywhere: `impl-app/src/host/types/tap_event.rs` has exactly Reading/Fault/Published, and `lib-persist`'s `LogRecord.payload` is rendered post-parse JSON, not pre-parse wire bytes. |

### TMS data models

| Gap | Status | Severity | Detail |
|---|---|---|---|
| **AC measurement models** | missing | high | *Deliberate descope (STOR-only), unchanged.* No AC-side types (frequency, real/reactive power, phase angle) added this merge; `lib-schema/src/lib.rs`'s scope doc comment is unchanged. |
| **ControlHardwareUpdate + start/stop & port-state models + 7 of 9 TMS enums** | missing | medium | Unchanged. Only `DtcSeverity`/`FailureModeIndicator` exist; none of `EnergyStartStopState`/`PowerPortState`/`DeviceRole`(as a schema type)/`CircuitWiring`/`EngineOperatingState` were added. |
| **DeviceInfo family** | partial | medium | *Narrowed from missing/high.* Now genuinely constructed and published every 60 s + at startup (`bin-rpi/src/tasks/heartbeat.rs::build_device_info`) over a live MQTT client — the analyst's original "never emitted" claim is refuted. Real gap that survives: the schema itself omits `DeviceRole`, `ControlHardwareInfo`, and `PowerHardwareInfo` wholesale versus baseline's `device_info.py`, and thins `ProductInfo`/`PowerPortInfo`/`TopicInfo` to a few plain fields each. `bin-rpi/src/config/types/device_config.rs`'s `role` field is `#[allow(dead_code)]` with a doc comment stating `lib_schema::DeviceInfo` has nowhere to carry it — the concept is config-representable but not wire-representable. (Note: baseline's own `role`/`DeviceInfo` construction is equally unwired at runtime, so this is a schema-breadth gap, not a "baseline does this and we don't" gap.) |
| **PowerHardwareUpdate subsystem breadth** | partial | medium | Unchanged. Still only `device_id`/`timestamp`/`energy_storage`; baseline models Engine/Fuel/Generator/PowerElectronics as well. Intentional battery-only narrowing, documented in `docs/tms-compliance.md`. |

### TMS transform

| Gap | Status | Severity | Detail |
|---|---|---|---|
| **AWS-Greengrass legacy schema transform** | partial | high | *New this run.* `lib-mapping/src/legacy/` + `bin_rpi_platform::GreengrassTap` (moved from `impl_app::host` under ADR 0009) genuinely map `CanonicalReading`/`CanonicalFault` to flat legacy structs and publish to `legacy/{device_id}/{kind}` — real, wired code (modulo the dispatch stub noted under Scope D / Publish targets). But direct field-by-field comparison against the real `example/aws/.../battery_telemetry.py`/`battery_reader.py`/`config.yaml` (present and checked into the baseline repo, contrary to this module's own "unavailable to check against" doc comment) shows **confirmed** divergence: two split MQTT messages vs. one merged payload; different field names (`state_of_charge` vs. `soc_percent`, etc.); array-shaped cell/temp fields vs. the real pipeline's flat per-cell keys; nested `LegacyLocation` vs. the real pipeline's flattened lat/long keys; and an assumption that `total_voltage`/`rated_capacity` are always populated when the real device config has both `enabled: false`. A downstream consumer built against the real legacy schema would not parse this output. Severity raised from an initial "unverified risk" framing to **high** because this is now a confirmed mismatch, not a speculative one. |
| **Runtime transformer registry / dispatch extensibility** | partial | low | Unchanged. Fixed 2-variant `TmsTopic` enum + hand-written `serialize_*` functions; the 4 new topics added this merge (StorageUpdate, Heartbeat, DeviceInfo, Reply) all bypass this enum entirely via their own ad hoc publish helpers — additional evidence for the existing finding, not a counterexample. |
| **Authored TMS schema breadth** | partial | low | *Narrowed materially.* 7 topics now authored-and-wired end to end (up from 3): Dc­MeasurementUpdate, PowerHardwareUpdate, ActiveDiagnosticState, StorageUpdate, Heartbeat, DeviceInfo, Reply. Still missing AC-side models, Engine/Fuel/Generator/ControlHardwareUpdate, start/stop/port-state enums, `Curve2DPoint` — a documented, deliberate STOR-only descope, not an oversight. |

### Publish targets

| Gap | Status | Severity | Detail |
|---|---|---|---|
| **Meshtastic mesh-radio forwarding** | partial | medium | *Narrowed from missing/high.* A real `lib-mesh` crate now condenses, frames (hand-rolled Meshtastic protobuf, self-flagged PROVISIONAL/unverified against real hardware — a field-hardware risk, same category as the RS-485 direction-control finding), and sends over a real serial port with backoff/rate-limiting; wired from `runtime.rs` and `impl_host_tap.rs::HostTap::Mesh`. Diverges from baseline: greedy-condense stops permanently at the first oversized field instead of trying smaller ones; the envelope omits `device_id`/`topic`; and **faults are never forwarded over mesh at all — a genuine regression**, since baseline's fault transformer does flow through its generic publish fan-out to the Meshtastic publisher today. |
| **Raw serial-capture-to-disk publisher** | missing | medium | Same underlying gap as the Protocols-dimension row above (severity reconciled to medium in both places). No tee-to-disk of pre-parse wire bytes anywhere in `lib-io` or `lib-persist`. |
| **MQTT knobs: topic slugging · local-broker QoS · reconnect observability** | partial | low | Unchanged — none of the touched files in this merge relate to it. Slugging still lacks acronym-run merging (currently invisible only because no live topic name has back-to-back capitals); QoS still hardcoded on the local-broker leg; remote-reconnect still delegated to the external mosquitto bridge. |

*Cross-reference note:* **Publisher fan-out with per-target isolation** (scored superior — see "Where sentient-rs is ahead") and the **Scope D dispatch stub** (scored partial/high under Ops, tests & docs, below) are the same code but score two different things: the fan-out *mechanism* (`SinkSet`/`HostTap`, non-blocking `try_send`, per-tap failure isolation) is genuinely sound and superior to baseline's blocking `asyncio.gather`. The *Greengrass consumer specifically* is wired into that mechanism but its dispatch arm is a no-op — so the mechanism being good does not mean Scope D is met. Read them together, not in isolation.

### App runtime & config

| Gap | Status | Severity | Detail |
|---|---|---|---|
| **device_type/role config field** | partial | low | `DeviceConfig.role` field added (default `"STOR"`), but marked `#[allow(dead_code)]` because `lib_schema::DeviceInfo` has no `role` field to carry it into — the wire schema can't represent it yet. Narrower gap than it first appears: baseline's own `role`/`DeviceInfo` construction is equally unwired at runtime (grepped — zero call sites beyond the model definition), so this is not a case of Rust trailing an operative baseline feature, just a symmetric, narrow gap on both sides. |

### Ops, tests & docs

| Gap | Status | Severity | Detail |
|---|---|---|---|
| **Contract Scope D overclaim** | partial | high | *New finding this run.* `impl-app/src/host/impl_host_tap.rs`'s `HostTap::Greengrass` arm is `log::trace!("greengrass tap: offer stub...")` — it never calls `tap.offer()`. `runtime.rs::build_taps` does construct and push a real `GreengrassTap` into the fan-out list, so every event is silently received and dropped. The only test on this path, `test-e2e/tests/legacy_greengrass_e2e.rs`, constructs a bare `GreengrassTap` and calls `.offer()` directly — bypassing `HostTap`/`SinkSet`/`build_taps` entirely, so it proves the renderer works but cannot catch the missing dispatch. Net: in the real shipped binary, `legacy/#` never receives a publish. `docs/audit-2026-07.md`'s NOT MET verdict for Scope D is still functionally correct; the merge commit and CHANGELOG overclaim closure. |
| **Lightweight dev deploy loop** | partial | low | Unchanged. No `deploy`/`sync`/`resync` recipe in `justfile`; `docs/deployment.md` §3 still describes the manual verify→copy→`systemd-sysext refresh` flow. Not touched by this merge. |

### Security & deployment

| Gap | Status | Severity | Detail |
|---|---|---|---|
| **Location service (`lib-locate`)** | partial | medium | *New to this run's gap table (previously noted only as wholly absent, under "Legacy AWS pipeline").* A real, tested 4-tier chain now exists (Manual → GpsNmea → ModemManager → IpGeo, `lib-locate/src/impl_chain.rs` + per-tier impls), wired via `runtime.rs` into `GreengrassTap`. This materially closes what was a total gap — but the raw analyst's "superior" call is corrected here: baseline's `location_service.py` runs a **6**-tier chain, adding **Cloud Override** (Greengrass thing-attribute override, highest priority) and **Cell-Tower** (Mozilla Location Service via 3GPP cell-tower D-Bus data) ahead of/between the four Rust tiers — confirmed via grep (zero hits for Cell/cloud_override/Mozilla anywhere in `lib-locate`/`bin-rpi`/`impl-app`). Real, well-built progress; not yet full parity. |

**Note on verification depth in this dimension:** the other 5 Security & deployment items this run (lib-security provider seam, CMS+cosign+cargo-vet+SBOM, Zephyr MCU completeness, Yocto deployment/hardening, and the RS-485 DIP scanner) are carried forward from the 2026-07-15 report. Four of them (security stack, supply-chain/signing, Zephyr MCU, Yocto hardening) were confirmed only via `git show 11b165f --stat` showing no changes under their paths — they were **not independently re-examined** this cycle beyond that diff check, unlike every other item in this report. Only the location-service and RS-485-scanner findings were freshly, independently re-verified this run. Treat the first four as "still true as of last week, untouched by this merge," not as freshly re-derived.

---

## Where sentient-rs is ahead

Newly superior/parity this run, on top of the prior report's list:

- **Runtime liveness & control-plane behavior:** a real 1 s Heartbeat and a real sub-100ms Request/Reply responder now run in production where baseline never wired either at all, plus a real, rotating, disk-full-tolerant JSON-lines telemetry log that exceeds baseline's unbounded single-file logger.
- **Shutdown correctness:** genuine SIGTERM interception + bounded drain, proven against a real spawned binary and a real signal, versus baseline's typical bare `KeyboardInterrupt` handling.
- **Config hardening:** every config section now rejects unknown keys, closing a class of silent-typo misconfiguration baseline doesn't guard against either.
- **Multi-bus fan-in:** one `HostPlatform`/task per configured `[[bus]]`, sharing one broker, aggregated into one systemd watchdog feed — a structured `tokio::JoinSet` analog to Python's thread-per-interface `InterfaceManager`, arguably cleaner (per-bus preflight, bounded graceful drain).
- **Field tooling:** `bin-rpi --scan-bus` is a genuine, tested DIP-address sweep reusing the production Seplos decode path — and in the process of comparing it, found a real off-by-one bug in the baseline's own scanner (`for dip in range(24)` against a documented 0–15 range).
- Everything the prior report already listed under this heading (core-path rigor, security stack, Zephyr MCU second target, Yocto deployment/hardening) is unchanged and, per the note directly above, was diff-checked rather than fully re-derived this cycle.

---

## Legacy AWS pipeline — not re-homed

Updated from the prior report given this run's findings:

- **Inverter telemetry over RS-232 — the second METEOR device class.** Unchanged: no telemetry path exists in sentient-rs. **New:** this is now a formally recorded decision — `docs/decisions/0001-rs232-inverter-leg-in-scope.md` (Status: Locked, dated 2026-07-16; supersedes the earlier descope call) — rather than a silent gap. The acronym contradiction between `main.py:135-147` (inverter=TPM/battery=TEM) and `recipe.yaml:14` (the reverse) is unchanged and still unresolved; the interface mapping remains the unambiguous part.
- **Multi-tier location service** — **substantially closed this run**, not absent as previously reported. `lib-locate` now implements 4 of the baseline's 6 fallback tiers (Manual, GpsNmea, ModemManager, IpGeo); Cloud Override (Greengrass thing-attribute) and Cell-Tower (Mozilla Location Service) remain unported. Move this line item's residual scope to "decide, don't drift" below rather than "not re-homed."
- **Greengrass local IPC publish** (`awsiot.greengrasscoreipc`) — still the real mechanism behind Scope D, and still distinct from what was built: the new Rust bridge publishes to a local MQTT topic (`legacy/{device_id}/{kind}`), not actual Greengrass IPC, and (per the Scope D findings above) doesn't even reliably do that yet.
- **Runtime hardware auto-detection** — unchanged, not touched by this merge; sentient-rs still assumes configured addresses.
- **Standalone RS-485 DIP-address bus scanner** — **closed this run.** `bin-rpi --scan-bus` is a real equivalent (and, per the field-tooling bullet above, catches a bug the baseline's own scanner has). Move out of this "not re-homed" list.

---

## Recommended path to parity

Re-ordered given this run's findings — contract exposure first, then the newly-discovered claimed-vs-actual gap, then field risk, then TMS ambition:

1. **Fix the Scope D dispatch stub.** Wire `HostTap::Greengrass`'s arm to actually call `tap.offer()` in `impl_host_tap.rs` — the renderer and config already exist; this is a small, high-leverage change, and until it lands Scope D is not met regardless of what the merge claims.
2. **Reconcile the legacy schema against the real reference.** Fix topic shape (one merged message, not two), field names, flat-vs-array cell/temperature keys, and location flattening against `example/aws/.../battery_telemetry.py`/`config.yaml` before re-claiming Scope D closure.
3. **Finish wiring RS-485 direction control end to end.** The mechanism (`RtsSoftware`/`KernelRs485`/`GpioDe`) is fully built and tested at the `lib-io` layer; connect `HostPlatform::build_port`/`reconnect` to `open_with_direction` instead of the plain `open` so the existing per-bus/global config actually takes effect. Low effort, closes a field-hardware risk that's been open two reports running.
4. **Validate Meshtastic against real hardware and restore fault forwarding.** The protobuf field numbers are self-flagged PROVISIONAL; verify them before field deployment. Add a fault path to `condense` — this is a regression versus baseline, not merely an unmet aspiration.
5. **Small TMS completions:** widen `DeviceInfo` (DeviceRole, ControlHardwareInfo, PowerHardwareInfo) if the MON-vs-power-device role question resolves toward needing it; AC measurement models and the remaining enums stay a deliberate descope until an AC-port module exists.
6. **Decide, don't drift:** the RS-232 inverter leg (now a locked decision — revisit if scope changes), the two missing location-service tiers (Cloud Override, Cell-Tower), the passive/framing layer, raw serial-capture-to-disk, and the dev-deploy-loop tooling gap. Each remains legitimate to descope — in writing, as with item 1 this cycle.
---

## Report footer — baseline SHAs for next run

| Repo | Ref | Commit SHA | Timestamp |
|---|---|---|---|
| `high-velocity-decisions/sentient-rs` | `origin/main` | `11b165fe689510350c2e023da1c2972320b2271a` | 2026-07-16T07:50:38-07:00 |
| `high-velocity-decisions/meteor-adapter` | `origin/master` | `f9db861` | unchanged this run |
| `high-velocity-decisions/sentient-rs` | `claude/gap-analysis-meteor-parity-n2o64c` (this branch) | (see branch HEAD after this commit) | 2026-07-16 |

Next scheduled refresh diffs both repos since the SHAs above.

---

## Addendum (2026-07-16, later same day) — gap-closure waves 1-3 landed (PRs #271/#272/#275)

Three further merges landed on top of `11b165f` since this report's base commit, closing several
items this report tracked as open or overclaimed:

- **Scope D — now genuinely closed.** Wave 2 (#272) fixed the dead `HostTap::Greengrass` dispatch
  stub this report flagged (`impl_host_tap.rs` now actually calls `tap.offer()`), proven by a real
  `SinkSet`-path e2e (closing the "claimed-vs-actual" gap class this report named — retrospective
  tracked as issue #269) — plus the legacy schema field-level reconciliation against the real
  reference pipeline. Wave 3 (#275) additionally routes the new RS-232 inverter leg's own
  telemetry/faults into the same legacy `inverter/{id}` payload.
- **The RS-232 inverter leg — built end to end.** Decision 0001
  (`docs/decisions/0001-rs232-inverter-leg-in-scope.md`, superseding the earlier descope) is now
  implemented: wave 2 (#272) added the Voltronic/PI30 parser (`lib-parse`, feature `voltronic`,
  PROVISIONAL — no real hardware capture); wave 3 (#275) wired the full poll loop, the CONV-role
  TMS mapping (`AcMeasurementUpdate`/`DcMeasurementUpdate`/`PowerHardwareUpdate`/
  `ActiveDiagnosticState`), and per-bus TMS identity (a unit can now run STOR + CONV roles
  concurrently, decision 0007) — see `docs/tms-compliance.md` §4.8.
- **RS-485 direction control — wired, not just built.** This report's "decision/connection needed,
  not a design gap" finding (config → `open_with_direction` was never called) is resolved: wave 1
  (#271) connects `HostPlatform::build_port`/`reconnect` to the configured `DirectionMode`, with a
  per-bus override over a top-level fallback (`docs/deployment.md` §4).
- **TMS IDL reshape.** Wave 1 (#271) re-baselined the entire schema against the checked-in
  `docs/compliance/tms/mil-std-3071_data_model.idl` (TMS_VERSION 2.0.2, not the 2.0.3 draft this
  report's "TMS data models" dimension was scored against) — `StorageUpdate`, `DeviceInfo`/
  `ProductInfo`/`TopicInfo`, `Reply`/`ReplyStatus`/`ReplyCode`, and the AC measurement family moved
  from this report's "missing/superior-but-PROVISIONAL" characterization to IDL-verified; the
  generic `Request` topic (an invention, never in the standard) was removed. Full detail, including
  what still doesn't conform (missing Required STOR publications, the MC-selection state machine,
  cadence/QoS profile violations): `docs/tms-compliance.md`.
- **Raw serial-capture-to-disk** — this report's "missing/medium" gap (no tee-to-disk of pre-parse
  wire bytes) is now landed as a writer + config surface (`lib-persist::run_capture_writer`,
  `[capture]` in `bin-rpi`'s config); the `SerialTransport` decorator that would feed it from a live
  poll loop is a tracked follow-on (`lib-io`'s `persist` module — see the [API reference](./api-reference.md)).

**Still in progress, not yet merged:** PR #274 (the IDL descriptor-factory — a generated,
byte-identical-proven registry derived directly from the IDL, intended to eventually replace the
hand-transcribed `lib-schema` types as the schema's source of truth) and the broader
spec-conformance sweep this addendum's linked issues (#256-#270, #273) track — missing Required
STOR topic publications, the MC-selection machinery, and publication cadence/QoS fixes chief among
them. This report's scorecard/verdict above is **not re-run** for this addendum; treat the bullets
above as a pointer to what changed, not a re-scored dimension table.

---

## Addendum (2026-07-20) — weekly refresh: waves 4-5, IDL descriptor factory, crate re-layer

**Method for this refresh:** `high-velocity-decisions/meteor-adapter` has **zero movement** since
the last run — confirmed via `git log`/GitHub API: still 3 commits, HEAD `f9db861`, no tags, no
releases, one branch (`master`). Nothing to re-baseline on the Python side. On the sentient-rs
side, `origin/main` moved from the `11b165f`-equivalent addendum state to **`c58b9da`** (2026-07-20)
— 18 commits, most of it viz/CI/docs tooling out of scope for this comparison. This refresh is a
**targeted verification of the parity-relevant subset** (three code-reading agents, each re-reading
the actual current source rather than trusting commit/PR messages) — not a full re-run of the
47-item scorecard above. Items not covered here keep their prior verdict.

### Gaps CLOSED this refresh

- **Raw serial-capture-to-disk (tee pre-parse wire bytes)** — missing/medium → **parity**. Wave 4
  (PR #279, `3de2cc9`) added `CaptureSerial`, a `SerialTransport` decorator
  (`bin-rpi-platform/src/impl_capture_serial.rs`) that is now unconditionally the concrete type
  behind `HostPlatform::serial`, built in both `build_port` and `reconnect`
  (`bin-rpi-platform/src/impl_host_platform.rs:57,353`), fed from real `[capture]` TOML config via
  `runtime.rs:304-482`, and proven end-to-end by `test-e2e/tests/capture_e2e.rs` (real binary,
  real pty, asserts `capture.jsonl` byte-matches the wire). This was a genuine baseline advantage
  (meteor-adapter's `serial/protocols/raw_capture.py` + `RawCapture` framing strategy) — closes the
  same underlying item scored under both Protocols and Publish targets in the table above.
- **Missing spec-Required STOR publications** (issue #263) — CONFIRMED closed. Wave 5 (PR #281,
  `e16bbbe`) wires real publishers for `ActiveMicrogridControllerState`, `EnergyStartStopState`,
  `PowerPortState`, and `DcLoadSharingState` (`bin-rpi/src/tasks/state_publisher.rs:60-158`,
  spawned per-identity from `runtime.rs:394`), with `DeviceInfo`'s self-declared topic lists updated
  to match, and a real e2e (`spec_conformance_e2e.rs`) asserting all four hit the wire. This
  materially narrows the **TMS data models** dimension's "ControlHardwareUpdate + start/stop &
  port-state models + 7 of 9 TMS enums" gap — 2 of those (EnergyStartStopState, PowerPortState) plus
  DcLoadSharingState and AMCState are now live; still missing: `ControlHardwareUpdate`, `DeviceRole`
  as a schema type, `CircuitWiring`, `EngineOperatingState`.
- **MC-selection machinery absent** (issue #264, A.7.5) — CONFIRMED closed. Real timers (New MC 3s /
  Lost Active MC 6s / No MC 10s, `bin-rpi-platform/src/impl_timing_profile.rs:7-16`, unit-tested),
  a real selection state machine (`bin-rpi/src/tasks/impl_mc_selector.rs::evaluate`, 6 unit tests),
  heartbeat/device_info subscription with role-filtering, `ActiveMicrogridControllerState`
  publish-on-change wired through to the state publisher, e2e-proven
  (`synthetic_mc_drives_active_microgrid_controller_state`). `CONFIG_ON_COMMS_LOSS` is an honestly
  documented no-op (logged + doc-commented as pending a config subsystem), not a silent gap.
- **`DiagnosticTroubleCode` field order** (issue #273) — code-fixed, though **the GitHub issue
  itself is still open (bookkeeping lag, not a live defect)**. Commit `68194b2` reordered `hint` to
  last in `lib-tms/src/generated.rs`, matching the IDL exactly; the flat-wire emission chain in
  `generated_fields.rs` follows the same order.
- **`StorageCellState.state_of_charge` should be `Option<f32>`** (issue #278) — closed; the
  generated `lib-tms` type now carries the correct optionality.

### Gaps NARROWED but still open

- **Publication cadence/QoS violations vs. spec profiles** (issue #265) — **PARTIAL**, 4 of 5
  sub-claims confirmed: PowerHardwareUpdate/StorageUpdate now rate-limited to spec Periodic 10s
  (`impl_emit_cadence.rs:19,40-58`); DcMeasurementUpdate rides the poll cycle, now defaulted to the
  spec-nominal 1s (`config/impl_poll_config.rs:12`); DeviceInfo publishes At-Start with
  `retain=true`/QoS1 (`heartbeat.rs:314-321`, e2e-verified); Reply carries the spec `Reliable`/
  history-128 QoS profile (`lib-tms/src/generated_qos.rs:157-167`). **Not confirmed:**
  ActiveDiagnosticState's claimed "most-severe-first truncation" — the code truncates DTCs at
  `MAX_DTC=64` in fixed table-iteration order, not severity-sorted (`lib-mapping/src/transform.rs:
  463-488`, `wire/inverter/impl_active_diagnostic.rs:99-111`). Practically unreachable today (Seplos
  has ≤16 fault flags, inverter ~7, both far under the 64 cap), but the specific mechanism claimed
  in the issue doesn't exist in code — worth a follow-up ticket if the fault-table size ever grows.
- **IDL descriptor factory** (issue #274, PR #274 `1a5381f`) — now merged and **live**, not a
  parallel unused artifact: `lib-tms/src/generated*.rs` is 100%-generated from
  `docs/compliance/tms/mil-std-3071_data_model.idl` via `tools/tms-idl-codegen`, gated in CI
  (`just tms-codegen-check`), and is what `lib-mapping`/`bin-rpi` actually compile against — no
  leftover hand-written schema file remains anywhere in the tree. This also folds in the
  `lib-schema` re-layer (issue #282): the old `lib-schema` crate is gone, split into `lib-tms`
  (generated standard), `lib-canonical` (device model), `lib-mapping` (the bridge). The "Authored
  TMS schema breadth" gap (TMS transform, was partial/low) is further narrowed by this — the
  serializer path is now IDL-generated/chain-based rather than hand-written `serialize_*`
  functions — but the specific "extensibility without codegen" claim wasn't independently
  re-verified this pass, so that sub-gap stays open pending a closer look.

### Gaps newly found (narrow) / still confirmed open

- **CONV-role `DeviceInfo` describes only 1 of 4 real inverter power ports** (issue #277, open) —
  CONFIRMED still true: `heartbeat.rs::conv_device` still emits exactly one `PowerPortInfo`
  (port 0, AC output), unit-tested to assert `power_ports.len() == 1`. Note the originally-cited
  root cause is stale: `MAX_PAYLOAD` was raised 1664B → 2048B by `68194b2`, but the code still only
  emits one port, now framed as a deliberate design choice (full port topology is "described by the
  AC/DC MeasurementUpdate telemetry itself") rather than a hard budget wall — worth re-litigating
  since the original forcing constraint that justified the shortcut no longer applies as stated.
- **Meshtastic fault-forwarding regression** (issue #267) — CONFIRMED still open, both sides
  independently re-verified this pass. Baseline: `meteor-adapter/src/meteor_adapter/core/
  pipeline.py:66-73`'s `AdapterPipeline._publish` fans every `TransformResult` — including faults
  built by `transform/tms.py:419-491`'s `_transform_fault` — out to all publishers uniformly, and
  `publish/meshtastic.py`'s `MeshtasticPublisher.publish` has no type filtering, so faults
  genuinely reach the mesh radio today in the Python baseline. sentient-rs: the mesh tap (now at
  `lib-net/src/mesh/`, dispatch in `bin-rpi-platform/src/impl_host_tap.rs:48-75`) only matches
  `TapEvent::Reading`; `Fault`/`InverterFault`/`InverterReading` fall through unhandled, explicitly
  documented in-code against `#267`. Unchanged from the prior report — still a genuine regression,
  not yet scheduled. *(**Update 2026-07-27**: closed — see the new addendum below. Battery-pack
  fault forwarding is fixed; inverter fault/reading forwarding over mesh is still unhandled, a
  narrower residual of the same issue.)*
- **`SerialTransport` lacks `discard_input()`** (issue #268, open, not previously tracked in this
  report) — new, minor: after a failed/timed-out exchange, leftover RX bytes can desync the next
  `read_until`. The Voltronic continuation-read loop bounds the damage today; flagged by the team
  as "evaluate after field captures show whether desync occurs in practice," i.e. deliberately
  deferred, not overlooked. *(**Update 2026-07-27**: closed — see the new addendum below.)*

### Not scored (outside the baseline comparison, no meteor-adapter equivalent to compare against)

- **DDS/RTPS transport** (story #321, all 9 work packages DONE/closed) — full MIL-STD-3071 §5.4/D.1-D.5
  compliance landed beside MQTT: RTPS/UDP, IPv6, domain 0, XCDR2 via a `lib-dds` seam over `dust_dds`,
  verified against a live CycloneDDS probe (byte-oracle chain, QoS semantics, e2e `dds-interop` CI
  lane). MCU-side DDS returned a hardware **NO-GO** (issue #328: IPv6+UDP costs +3016B against only
  3056B free SRAM headroom on the current 64KB substrate) — report-only this round, no MCU product
  code. Not a "parity" item since meteor-adapter has no DDS path at all (MQTT-only); mentioned here
  as a new sentient-rs-only capability, not a scored gap closure. *(**Update 2026-07-27**: the NO-GO
  itself is superseded — see the new addendum below. `CHANGELOG.md` was not updated to match and is
  now stale on this point.)*
- HIL Pico Seplos BMS pack-emulator firmware (#334) and the MQTT visualization/Grafana stack
  (#298-#319) are test/observability infrastructure improvements with no baseline equivalent to
  compare against — noted for completeness, not scored.

### Crate re-layer (file-path bookkeeping only — no behavior change)

Several paths cited earlier in this report moved during a crate-graph fitness cleanup (#282, #285,
#286, #287) that landed between the last addendum and this one. For anyone following this report's
file citations against current `main`:

| Report citation | Now |
|---|---|
| `lib-schema` | Split: `lib-tms` (generated MIL-STD-3071 types), `lib-canonical` (device model), `lib-mapping` (canonical→TMS bridge, unchanged name) |
| `lib-persist` | Folded into `lib-io/src/persist/` |
| `lib-preflight` | Physically moved to `spec-app/src/preflight/`; `impl_app::preflight` still resolves via re-export |
| `lib-mesh`, `lib-locate`, `lib-link`, `lib-net-embassy`, `lib-channel` | Consolidated into `lib-net/src/{mesh,locate,link,embassy,channel}/` |
| `impl-app/src/host/impl_host_tap.rs` | Moved to `bin-rpi-platform/src/impl_host_tap.rs` (ADR-0009 externalize bin-rpi-platform); `impl-app/src/host/` no longer exists |
| `bin_rpi_platform::GreengrassTap` | Unchanged public path (`bin-rpi-platform/src/types/greengrass_tap.rs`) |

### Updated "Recommended path to parity" status

Of the 2026-07-16 report's 6-item list: **items 1-3 are done** (Scope D dispatch fix, legacy schema
reconciliation, RS-485 direction-control wiring — all landed in waves 1-2, confirmed in the prior
addendum). **Item 4 (Meshtastic fault forwarding + hardware validation) is still open**, confirmed
above. **Item 5 (TMS completions)** partially addressed — EnergyStartStopState/PowerPortState/
DcLoadSharingState/AMCState landed via wave 5, but `DeviceInfo` breadth (DeviceRole,
ControlHardwareInfo, PowerHardwareInfo) is unchanged, and the CONV-role port-count gap (#277) is a
new wrinkle on the same theme. **Item 6** (RS-232 inverter leg is now done per wave 2-3; location
service tiers, framing layer, dev-deploy-loop remain open, not re-verified this pass).

### Report footer — baseline SHAs for next refresh

| Repo | Ref | Commit SHA | Timestamp |
|---|---|---|---|
| `high-velocity-decisions/sentient-rs` | `origin/main` | `c58b9da` | 2026-07-20 |
| `high-velocity-decisions/meteor-adapter` | `origin/master` | `f9db861` | unchanged since 2026-07-16 (and before) |

Next scheduled refresh diffs both repos since the SHAs above.

---

## Weekly refresh — 2026-07-27

`meteor-adapter` has **zero movement** since every prior run — reconfirmed via `git log`, the
GitHub releases API, and a pull-request search: still 3 commits, HEAD `f9db861`, no tags, no
releases, no PRs open or merged, ever. Nothing to re-baseline on the Python side, again.
`sentient-rs` moved from the prior refresh's `c58b9da` to **`de08b0c`** — 31 commits. The large
majority (RP2350 on-target profiler, MCU chip-matrix CI, Robot-library restores, Harbor registry
migration, viz/HIL tooling) is MCU-profiling and CI infrastructure with no meteor-adapter
equivalent to compare against, so it is not scored. This refresh is a **targeted verification of
the parity-relevant subset** — each finding below was re-checked directly against current source
(`git show <sha> --stat` plus reading the actual code), not inferred from commit messages — not a
full re-run of the 47-item scorecard. Items not covered here keep their prior verdict.

Housekeeping note: this report's canonical location moved during this refresh from
`docs/gap-analysis-meteor-parity.md` to `docs/reports/gap-analysis-meteor-parity.md`, matching the
`main`-branch docs reorg (#371) that relocated it several weeks ago. This branch's copy had not
picked up that rename until now, since this automation's own file continued to accumulate at the
old path across refreshes while `main`'s copy (edited independently in the interim) moved. The two
copies have diverged in framing in places; this refresh continues from **this branch's** history
(the designated base for this recurring job) and re-verifies every claim below directly against
current code rather than reconciling the two narratives line-by-line.

### Gaps closed this refresh

- **Meshtastic fault-forwarding regression** (issue #267) — **CLOSED**. Landed in `de08b0c`
  (2026-07-26, PR #397, sub-issue #387). `HostTap::Mesh` (`crates/bin/rpi-platform/src/
  impl_host_tap.rs:49-102`) now matches `TapEvent::Fault` explicitly: it looks up the last cached
  reading for that DIP and forwards a condensed fault frame immediately rather than waiting for the
  next reading cycle. `lib_net::mesh::CondenseInput` gained an `fault: Option<&CanonicalFault>`
  field, encoded as a compact JSON bitmask key `"f"`, exercised by new unit tests
  (`reading_then_fault_forwards_a_condensed_fault_frame_immediately` and siblings). This closes the
  regression this report has tracked since 2026-07-16. **Bundled fix, more serious than the
  originally-tracked issue:** `MeshPacket.to` was never encoded (#386) — Meshtastic firmware's
  `Router::sendLocal()` logs an error on `to == 0` and nothing rewires it to broadcast, so frames
  built before this fix were likely **never delivered at all**, not merely "unverified." `to` now
  emits `15 FF FF FF FF` (field 2, wire type 5, byte-verified against the firmware-derived
  sequence) — a strictly worse finding than this report's prior framing, now fixed alongside it.
  **What's still open:** inverter fault/reading forwarding over mesh remains unhandled (explicitly
  scoped out in-code, a narrower residual of #267), and **HIL validation against a real Meshtastic
  radio has not happened** — the PR's own verification notes say so plainly. Say "fault forwarding
  fixed, HIL pending," not "verified."
- **`SerialTransport` lacks `discard_input()`** (issue #268) — **CLOSED**. Commit `9390d84`
  (merged 2026-07-23) added a `discard_input()` method to the `SerialTransport` trait
  (`crates/lib-io/src/transport/types/serial_transport.rs`), with host implementations
  (`impl_rs232.rs`, `impl_rs485.rs`, `impl_host_serial.rs`) backed by `tokio_serial`'s
  `ClearBuffer::Input`, wired into three call sites in the read-exchange error path
  (`crates/lib-io/src/driver/mod.rs:55,93,148`). `CaptureSerial` forwards the call to its inner
  transport, so the raw-capture decorator added in the prior refresh doesn't bypass it. This was
  the smallest of the three open items on this report and is now fully closed.

### Newly found this refresh

- **`CHANGELOG.md` is stale on the MCU DDS NO-GO (issue #328)** — *new finding, not a parity gap
  but the same "claimed-vs-actual" risk category this report has named since 2026-07-16.*
  `CHANGELOG.md`'s Unreleased → Features section still reads "the MCU stays MQTT/JSON-only — DDS is
  Pi-only (MCU feasibility spike NO-GO on the 64KB substrate, #328)." That's no longer true: commit
  `33d48a2` (2026-07-24, HIL-verified) shipped MCU-side DDS/RTPS by adding an external WIZnet W5500
  SPI-to-Ethernet chip to the RP2350 board, giving it a real L2 driver and an `embassy-net` IP
  stack that sidesteps the original NO-GO's SRAM/IPv6 constraint (which was about the in-SoC
  stack, not an external NIC). SPDP participant announce and `DcMeasurementUpdate` over DDS
  (RTPS DATA+SEDP, XCDR2) are both live and interop-checked against a real CycloneDDS probe
  (`crates/bin/mcu-rp2350/ns/src/{spdp,dds_pub}/`). Neither `33d48a2` nor the subsequent `de08b0c`
  touched the stale `CHANGELOG.md` line. This isn't a scored gap (meteor-adapter has no DDS path to
  compare against — still correctly "not scored" below), but it's worth flagging in the same spirit
  as the Scope D overclaim this report opened with: a project doc asserting a capability boundary
  that shipped code has already crossed.

### Confirmed still open, unchanged

- **CONV-role `DeviceInfo` describes only 1 of 4 real inverter power ports** (issue #277) —
  untouched since the last refresh. `git log c58b9da..de08b0c -- crates/bin/rpi/src/tasks/
  heartbeat.rs` shows only a pure directory-reorg commit (`fc57157`, crates → `crates/bin/`) with
  no functional diff. `heartbeat.rs:436` still asserts `power_ports.len() == 1`.
- **ActiveDiagnosticState's severity-truncation claim** — untouched; no commit since `c58b9da`
  touches `lib-mapping/src/transform.rs`. `MAX_DTC=64` truncation remains fixed table-order, not
  severity-sorted, as this report has noted since 2026-07-20.
- **Greengrass/legacy schema, location-service tiers** — untouched this refresh; no commits beyond
  the pure crate-reorg (`fc57157`) touch either path.

### Not scored — no baseline equivalent to compare against (new this refresh)

Landed together in `de08b0c` (PR #397), explicitly framed by its own commit message as "four
additions beyond the contracted scope, for handover" — the Seplos → TMS → MQTT mission path is
unchanged by any of these:

- **Adapter self-telemetry (INA3221)** — the Pi's own rail draw, published to a non-TMS
  `diag/rails` topic and shown on-device. Deliberately kept out of `DcMeasurementUpdate` (an earlier
  pass had wrongly conflated it with battery-pack telemetry; this landed the correction). No
  baseline equivalent — meteor-adapter has no device-self-monitoring story.
- **On-device UI** — a 128×32 SSD1306 OLED, four screens, golden-tested against a mock display, no
  hardware required to verify. No baseline equivalent.
- **Board port** — Pico 2 → Adafruit RP2350 Feather (flash window 4MB → 8MB, full pin remap). An
  MCU hardware-target change with nothing to compare on the Python side.
- **Harbor CI registry migration** — pure build infrastructure (the internal container registry
  was rebuilt on a new host with a new CA); no product-code or parity relevance at all.

None of these move the scorecard; they're noted here purely so a future refresh doesn't mistake
`diag/rails`, the OLED UI, or the Feather board for an undocumented parity claim.

### Report footer — baseline SHAs for next refresh

| Repo | Ref | Commit SHA | Timestamp |
|---|---|---|---|
| `high-velocity-decisions/sentient-rs` | `origin/main` | `de08b0c` | 2026-07-26 |
| `high-velocity-decisions/meteor-adapter` | `origin/master` | `f9db861` | unchanged since 2026-07-16 (and before) |

Next scheduled refresh diffs both repos since the SHAs above.

---

## Refresh — 2026-07-27 (later same day)

Triggered by new work landing within hours of the refresh above. `meteor-adapter` is, again,
**unchanged** — reconfirmed via `git log`, the releases API, and a PR search: still `f9db861`, no
tags, no releases, no PRs, ever. `sentient-rs` moved from this morning's `de08b0c` to
**`79463fd`** ("Delight features, MIL-STD-3071 enforcement, transport-security seam, CI cost",
PR #399) — a single large commit, 128 files, +7379/-492. Verified directly against current source
by three independent passes (one per theme below), not inferred from the commit title.

### Gaps closed / hardened this refresh

- **MIL-STD-3071 cadence enforcement catches a real, already-shipped regression.** New
  `PollConfig::validate()` and `DdsToml::validate()` (`crates/bin/rpi/src/config/impl_poll_config.rs`,
  `impl_dds_toml.rs`) reject, at config-load/boot time, any `interval_ms` outside `1..=1000` (the
  spec's 1 Hz `DcMeasurementUpdate` requirement, §4.9.10.4) and any non-zero DDS `domain_id`
  (Annex D.2), wired into `runtime.rs::supervise` and `dds_config()` so a bad config now fails to
  boot instead of silently under-publishing. This isn't just new validation — the diff shows
  **every** shipped example config (`config/sentient.toml`, both `deploy/` configs, both `distro/`
  configs) actually had `interval_ms = 2000` (0.5 Hz, a real spec violation) before this commit
  corrected all of them to `1000` and added the guard so it can't regress again. This tightens
  (does not newly close) the "Publication cadence/QoS violations vs. spec profiles" item this
  report has tracked since 2026-07-20 — that item's own DcMeasurementUpdate sub-claim should be
  read as "now enforced," not merely "now correct by convention."
- **RS-485 direction-control wiring reconfirmed intact.** `crates/bin/rpi-platform/src/
  impl_host_platform.rs`'s `build_port` (line 39) and `reconnect` (line 345) both still call
  `HostSerial::open_with_direction(...)` — this file has zero diff in this commit, so the "wave 1"
  fix this report has tracked as closed remains closed, not silently reverted by the large diff
  around it.

### New this refresh — not scored, no baseline equivalent

- **Transport-security seam (`crates/lib-security`, `crates/lib-tls`) — an honestly-staged
  skeleton, not a shipped capability.** Two new crates implement five new traits from ADR 0013
  (`docs/decisions/0013-transport-security-trust-store.md`) — `TrustStore`, `KeyAgreement`,
  `KeyDerivation`, `RecordCipher`, `TrustedTime` — but their only implementation anywhere is
  `backends::unimplemented`, whose module doc states the rule directly: **"No stub may return
  success."** Every method returns `Err(...NotImplemented)`. `lib-tls`'s own header is equally
  direct: **"Status: SKELETON. Nothing here performs cryptography, and nothing here can authorize a
  peer."** Confirmed no runtime call site anywhere: `bin-rp2350/ns/Cargo.toml` states `lib-tls`
  "is NOT consumed here yet, because there is nothing here that could call it," and the actual TLS
  insertion point, `boot_tls.rs::interpose_tls()`, unconditionally aborts every socket rather than
  calling into the new crate ("fabricating a call here to look real would be theatre," per its own
  comment). `.ci/crate-graph.toml` tracks `lib-tls` as known debt under issue #398. ADR 0013 itself
  is marked **"Status: Proposed (2026-07-26)... no code written yet"** even though this scaffolding
  landed alongside it — its own staged plan (WP2–WP8) puts a real backend, real `lib-tls`/rustls
  wiring, and hardware identity-key provisioning all in the future, stating plainly "no honest
  security claim is possible before this lands." Read together, this is the **opposite** of the
  Scope D overclaim pattern this report opened with — new capability is described with more
  caution than the code alone would justify, not less. Not scored (meteor-adapter has zero
  TLS/mTLS story), but worth recording given the scale of the addition. Contract **Scope E (TPM
  security hardening) is unchanged**: `docs/security/scope-e-tpm-signing-report.md` is a separate
  topic (device/build key custody, not transport encryption), still explicitly consult-only —
  "the shipping code does not depend on a TPM" — and ADR 0013/0014 never reference it.
  ADR 0014 (`docs/decisions/0014-device-identity-and-config-provenance.md`, same "Proposed, no
  code written yet" status) is about what belongs in a device X.509 cert vs. a signed config
  artifact; it does not touch `DeviceInfo`/`role`, so it doesn't bear on that already-tracked
  schema-breadth gap.
- **New integrator-facing reference/compliance docs**: `docs/deployment/rs485-wiring.md` (187
  lines — transceiver selection, DE-pin tables, A/B bus-wiring guidance for a customer laying out
  their own board; candidly notes the physical RS-485 layer is bench-unproven on either target, no
  real transceiver in the HIL loop yet), `docs/reference/chipset-compatibility.md` (392 lines —
  supported-MCU-silicon survey, RP2350 vs. STM32H5/U5/nRF5340, FIPS/CMVP survey, port-cost
  estimate), `docs/reference/reference-bom.md` (377 lines — component BOM/pricing, flags the
  Heltec mesh radio's China-origin export-compliance exposure), and `docs/compliance/
  taa-applicability.md` (275 lines — Trade Agreements Act scoping for a customer's own board).
  None reference RS-232/Voltronic or the Meshtastic protobuf-verification finding already tracked
  in this report — genuinely new content categories, not restatements. No meteor-adapter
  equivalent whatsoever (a Python serial bridge has no hardware BOM or compliance posture) — not
  scored.

### Confirmed unchanged this refresh

- **CONV-role `DeviceInfo` port gap (#277)** and **ActiveDiagnosticState severity truncation** —
  neither `heartbeat.rs` nor `lib-mapping/src/transform.rs` appears in this commit's diff at all.
  `device_config.rs`'s `role` field is likewise untouched — still `#[allow(dead_code)]`.

### Report footer — baseline SHAs for next refresh

| Repo | Ref | Commit SHA | Timestamp |
|---|---|---|---|
| `high-velocity-decisions/sentient-rs` | `origin/main` | `79463fd` | 2026-07-27 |
| `high-velocity-decisions/meteor-adapter` | `origin/master` | `f9db861` | unchanged since 2026-07-16 (and before) |

Next scheduled refresh diffs both repos since the SHAs above.

# KICKOFF — sentient-rs

New **production** Rust project for the Sentient METEOR serial/BMS telemetry adapter. This is a
ground-up, production-grade build — **not a line-by-line port**. The existing Python
(`../meteor-adapter`) is a **prototype reference** that shows *how the interaction works* (the
observed Seplos wire exchange, the poll sequence, the field decode); it is **not the ground
truth and not the spec**. Ground truth is the **Seplos BMS protocol + real device behaviour +
the TMS standard**. Designed from day one for the right hardware abstraction and hardening
(**Zephyr**-grade), delivered on **Raspberry Pi**, forward path to a dedicated embedded MCU.

## Goals (from HVD → Sentient proposal, 2026-06-15)

Port/formalize Sentient METEOR **TPM/TEM** telemetry from AWS-dependency to a portable,
secure, **Tactical Microgrid Standard (TMS, MIL-STD-3071)**–compliant architecture. Contract
deliverable is **production-ready Rust** running on the **local Raspberry Pi comms module at
each TPM/TEM**, each hosting a **local MQTT server** for that device's TMS data. Fixed price,
**14-day** sprint.

Scope of work (A–E):
- **A. Demo & baseline** — local microSD data logging demonstrated; existing pipeline analysis finalized.
- **B. Private-cloud integration** — modify the MQTT transport to route TPM/TEM telemetry to
  designated **private-cloud MQTT endpoints**; remove reliance on **AWS IoT Greengrass + Kinesis**.
- **C. Standards compliance** — refactor the data payload to the **TMS** data model.
- **D. Backwards compatibility** — also deliver data back over **AWS IoT Greengrass** so existing
  flows keep working (transparent integration).
- **E. Security hardening (supported, ≤10h consult)** — hardware **TPM** for crypto/signing so
  data-in-transit is protected and the binary is verifiably trusted; HVD recommends TPM options.

Deliverables: compiled RPi binaries (local MQTT server per device, TMS data) · production-ready
Rust source · brief deployment guide · final TMS-compliance test/validation report.

Timeline (notional, relative to kickoff): TMS compliance schema → W1D2 · private-cloud MQTT
redirect → W1D4 · AWS backwards-compat → W2D1 · local-storage demo + baseline sign-off → W2D3 ·
final joint T&V + deployment hand-off → W2D5.

Day-1 dependencies (from Sentient): private-cloud endpoint credentials + a configured test
environment. Assumes current METEOR data maps to TMS **without** changing the underlying data
structure (resolving non-compliant METEOR structures is out of scope).

## Execution model (orchestration)

- **Start = PLAN, by Fable.** Open each round in plan mode; **Claude Fable 5** produces the plan
  (crate work, slice contract, test strategy) before any code.
- **Execution = Opus orchestrates.** After the plan is approved, **Claude Opus** orchestrates the
  build, delegating to:
  - **the gen model** (fine-tuned `pd` adapter in `../llm-finetune-rust-embedded`) — the
    embedded-best-practices **PLANNER** / REASON stage,
  - **Haiku** — the body **FILL** / COMPLETE stage,
  - the deterministic **gate** (`cargo check` + completeness) as ground truth.
- This is the enforced planner→scaffold→Haiku-fill→gate loop, orchestrated by Opus, planned by
  Fable. No slice is hand-rolled around it.

## Fill this in each round

```
SCOPE (this round's slice):
  <the one module/behaviour to build this round — e.g. Seplos frame parser, TMS payload
   mapping, private-cloud MQTT publisher, microSD logger, AWS Greengrass bridge>
```

Fixed context (decided):

```
PROJECT DIR:     D:\hvd\sentient-rs   (this repo — create the workspace here)
LANGUAGE:        Rust (production deliverable)
DELIVERY TARGET: Raspberry Pi comms module at each TPM/TEM (Linux binary) — the CONTRACT target
FORWARD TARGET:  dedicated embedded MCU later (not in this contract's scope)
ABSTRACTION:     Zephyr-grade discipline from day one — portable no_std-friendly core behind
                 trait transports, so the RPi→MCU move is a transport swap, not a rewrite.
                 Stand up a Zephyr `west` build as an EARLY SECONDARY target to de-risk +
                 harden; it does NOT replace the RPi Linux deliverable. Zephyr apps build via
                 `west` (Cargo as a subdirectory build), never `cargo build` directly.
GROUND TRUTH:    Seplos BMS protocol + real device captures + the TMS (MIL-STD-3071) standard
REFERENCE ONLY:  D:\hvd\meteor-adapter  (Python prototype — how the interaction works; NOT spec,
                 NOT a port target, read-only)
POSTURE:         production, not prototype — robustness, observability, config, hardening
```

**Platform note (read carefully):** the paid deliverable is a Linux binary on the RPi comms
module — do not block it on Zephyr. Zephyr is adopted for the *core architecture discipline*
and as a forward path; keep the protocol/TMS core platform-free so both targets consume it.

## Reference prototype (how the interaction works — NOT the spec)

`D:\hvd\meteor-adapter` — the Python prototype. Use it to **understand the observed behaviour**:
the METEOR device runs a **Seplos BMS** over **RS-485 @ 19200 8N1**, ASCII request/response
(`~VER+ADR+CID1+CID2+LENGTH+INFO+CHKSUM\r`; command codes 0x42 telemetry, 0x44 fault/warning,
0xA4 serial number), telemetry = cell voltages, temps, current, SOC/SOH, mapped to the
**TMS (MIL-STD-3071)** model and published to MQTT / Meshtastic.

Read these to learn the interaction, then **design the production Rust independently** — better
error handling, bounded memory, typed states, no prototype shortcuts:
- `src/meteor_adapter/serial/protocols/seplos_bms.py` — the Seplos exchange + field decode
- `src/meteor_adapter/serial/seplos_poller.py` — poll sequence / command ordering
- `src/meteor_adapter/serial/{framing.py,rs232.py,rs485.py,protocols/base.py}`
- `tests/test_seplos_*.py` — worked examples of frames/decodes (**illustrative vectors**, cross-
  check material — not the acceptance oracle)

**What is authoritative:** the Seplos protocol as observed on the wire + real device captures +
the TMS standard. Where the Python is unclear, ambiguous, or looks like a prototype hack,
**resolve against the protocol/standard, not against the Python.** Do not inherit its structure,
its bugs, or its shortcuts. Capture-derived test vectors (and any illustrative frames from the
Python) become Rust tests, but the **production correctness bar is the protocol + TMS**, not
Python parity.

## Data pipeline (what the system does)

```
Seplos BMS (RS-485) ──▶ framing/parse ──▶ TMS mapping ──▶ fan-out:
                                                          ├─ local MQTT server (per-device TMS)
                                                          ├─ private-cloud MQTT publisher
                                                          ├─ AWS IoT Greengrass (backwards-compat)
                                                          └─ microSD logger (local persistence)
```

## Crate layout (decided)

| crate | responsibility | I/O? |
|---|---|---|
| `sentient-core` | pure Seplos protocol (framing, parse, checksum) + device telemetry model. **TMS-agnostic.** Typed **structs** + the `bytes` crate for zero-copy frame decode. | none |
| `lib-mapping` | TMS (MIL-STD-3071) payload mapping: core telemetry ⇄ TMS schema, as typed **structs** encoded/decoded with the `bytes` crate. Python = field-mapping reference. Pure. | none |
| `lib-mqtt` | networking/comms: local MQTT server, private-cloud MQTT publisher, AWS IoT Greengrass bridge (+ Meshtastic if in scope). Behind traits. | yes |
| `lib-io` | serial transport (**RS-232 and RS-485**) behind one trait + a **factory** to select/swap; microSD persistence. | yes |
| `bin-rpi` | the RPi Linux **binary** (deliverable): wires core + mapping + net + io + config. | app |
| `bin-zephyr` | early Zephyr `west` target linking `sentient-core`/`-mapping`; stubs io/net HALs. | app |

## Data representation (structs + `bytes`)

Model frames and payloads as **typed Rust structs**, with byte-level encode/decode via the
**`bytes` crate** (`Bytes`/`BytesMut`, `Buf`/`BufMut`) — zero-copy where possible, explicit
field layout, no ad-hoc slicing. This covers the Seplos frame decode in `sentient-core` and the
TMS payload encode in `lib-mapping`. Use the Python only as a **reference for the field
mapping** (which byte means what, scale factors, endianness); the struct definitions and the
`bytes` codec are authored fresh to the protocol/TMS, not transcribed. `no_std`-friendly
(`bytes` supports `no_std` + `alloc`; if the MCU target forbids `alloc`, use `heapless` +
fixed-size struct codecs behind the same trait).

## Serial I/O abstraction (RS-232 + RS-485, factory)

One trait for the byte transport (open/configure/read/write; params: baud, parity, stop bits;
RS-485 adds driver-enable / half-duplex direction control). Two impls — `Rs232`, `Rs485` — and a
**factory** that constructs the right one from config (`SerialKind::Rs232 | Rs485` + params) and
returns it as the trait object, so switching RS-232↔RS-485 is a config change, not a code change.
Port `../meteor-adapter/src/meteor_adapter/serial/rs232.py` and `rs485.py` as the two impls and
`framing.py` into `lib-io`. The Seplos poller drives this trait, never a concrete port.

## Architecture principles

- **Layered for portability.** `sentient-core` (protocol) and `lib-mapping` (TMS) are
  `no_std`-friendly pure Rust with **zero I/O and zero platform surface**. Serial (RS-232/RS-485
  via the factory), MQTT (local server + private-cloud + AWS Greengrass), and microSD persistence
  live behind **trait interfaces** in `lib-io` / `lib-mqtt`, so the same core+mapping run
  on the RPi Linux target now and a Zephyr/MCU target later with only the impls swapped.
- **RPi is the paid target; Zephyr is the discipline + forward path.** Ship the RPi Linux binary;
  keep a Zephyr `west` build as an early secondary to harden (watchdog, fault handling, bounded
  memory) and de-risk the MCU move. Never let Zephyr block the contract deliverable.
- **Production posture, not prototype.** Robust error handling on every fault path, bounded
  memory, typed state machines, structured logging/observability, config over hardcoding, clean
  shutdown/recovery. Assume this ships and runs unattended at a TPM/TEM — build for that.
- **Tests derive from the protocol + captures + TMS, not from Python.** Each module lands with
  Rust tests built from real/captured frame vectors and TMS schema expectations (illustrative
  Python frames may seed them). `cargo test` (host) green is the bar for pure layers; on-target
  smoke for transports; TMS compliance validated against the standard's schema, not by eyeball.
- **Bounded, hardened core:** `heapless` buffers/queues, fixed frame sizes, typed errors, **no
  `unwrap` in logic** — required for the embedded forward path and good practice on RPi.
- **Config-driven endpoints/creds** (private-cloud MQTT, Greengrass) — injected, never hardcoded;
  Day-1 Sentient credentials go in config, not source.

## First steps for the round

1. Create the `sentient-rs` Cargo **workspace** here with the six crates in the **Crate layout**
   table above (`sentient-core`, `-mapping`, `-net`, `-io`, `-rpi`, `-zephyr`).
2. Read the SCOPE slice's Python reference **and its example vectors** to understand the
   interaction, then write the **authoritative contract from the protocol/TMS** (frames, checksum
   algorithm, field layout, TMS field mapping, every error/fault case) — not a transcription of
   the Python.
3. Build the slice through the trained planner → scaffold → Haiku fill → gate loop — **mandatory
   for every slice** (see the ENFORCED Codegen workflow section); capture the plan + gate verdict.
   Place it in the right crate (protocol→core, TMS→mapping, MQTT/Greengrass→net, serial/microSD→io);
   write Rust tests from captured/spec vectors + TMS expectations; `cargo test` green.
4. If the slice touches serial I/O, extend the `lib-io` transport trait + factory and provide
   the RS-232/RS-485 impls; if it touches comms, define the `lib-mqtt` trait + RPi impl and
   stub the Zephyr impl. Confirm the RPi binary builds and the Zephyr `west` skeleton builds.
5. Report: crate layout, slice built, tests + what vectors they came from, RPi + Zephyr build
   status, TMS-compliance note (if applicable), and the next slice against the timeline.

## Codegen workflow — the trained planner + Haiku fill (ENFORCED)

This project is the **intended consumer** of the model in `../llm-finetune-rust-embedded`.
That model was fine-tuned for exactly this role: **an embedded-best-practices PLANNER** — this
is the core reason it was trained. **This workflow is MANDATORY for EVERY slice — no exceptions,
no hand-authoring around the pipeline** (host-logic included: Seplos decode, TMS mapping, net,
io — all of it). The trained model plans (REASON), Haiku fills (COMPLETE). Follow
`docs/PIPELINE_USAGE.md`; the roles map directly:

- **REASON — the trained model is the planner (adapter `pd`, best reasoning grounding).** Given a
  typed intent + policy, it emits the applicable-principles checklist + implementation plan for
  the slice: `no_std`/`no_alloc` discipline, ISR-safe sharing (`critical_section`, not bare
  `static mut`), `StaticCell`→`&'static mut`, bounded/`heapless` memory, panic-handler choice,
  cancel-safety, watchdog. This best-practices reasoning is *the reason the model exists* — do
  not confiscate it with hand-rules; let it plan.
- **RETRIEVE** — per-platform rustdoc grounding (embassy/HAL APIs) folded into the plan.
- **SCAFFOLD** — the deterministic executor lowers the plan to a **compiling, policy-correct**
  Rust scaffold with `/* FILL#n */` body holes (types, task shapes, panic handler, ISR-safe
  statics all fixed; bodies deferred).
- **COMPLETE — Haiku fills the bodies.** Measured: Haiku clears the bar; local small coders
  (Qwen 7B-4bit / 3B-bf16) do **not** — use Haiku (or a frontier instruct model) for fills.
- **gate** — `cargo check` + completeness is ground truth; ≤3 replans.

**Enforcement:** **every** slice — embedded (Zephyr/MCU, `no_std` core, ISR/serial/HAL) **and**
host-logic (Seplos decode, TMS mapping, net, io) — MUST be produced through this
planner→scaffold→Haiku-fill→gate loop. The model plans the best practices, Haiku fills, the gate
proves it. Report the plan (applicable principles) and the gate verdict for each slice. **The
pipeline drafts; it does not ship unreviewed** — the production correctness bar (Seplos protocol
+ captures + TMS) and human review apply on top of a green gate.

## Guardrails

- **EVERY slice goes through the trained planner + Haiku fill — ENFORCED, no exceptions.** The
  fine-tuned model (adapter `pd`) is the best-practices PLANNER (REASON); Haiku does the body
  fill (COMPLETE); the compile+completeness gate is ground truth. Embedded and host-logic alike
  (Seplos decode, TMS mapping, net, io) — do not hand-roll any slice around the pipeline. This is
  why the model was trained. Local small coders are NOT acceptable fillers.
- **Correctness is measured against the Seplos protocol + real captures + the TMS standard —
  NOT against the Python prototype.** The Python shows how the interaction works; it is not the
  spec and not a port target. Don't inherit its structure, bugs, or shortcuts.
- **Production-grade only.** No prototype shortcuts, no `unwrap` in logic, no silent failure —
  every fault path handled, bounded memory, config-driven, observable.
- Keep the protocol core (`sentient-core`) + TMS mapping (`lib-mapping`) free of
  platform/transport surface so the RPi→MCU move is an impl swap, not a rewrite.
- Zephyr builds via `west`, never `cargo build` directly on the Zephyr app.
- `meteor-adapter` is read-only reference — do not modify it.
```

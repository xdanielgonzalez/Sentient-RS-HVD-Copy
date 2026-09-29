# 0003 — TMS device role = STOR (power device proxying the battery), not MON

**Status:** Locked (2026-07-16)

## Context

MIL-STD-3071 (the TMS standard) requires every network participant to declare a device role. Two
roles were live candidates for this adapter:

- **MON** — a passive monitor that observes and reports telemetry but is not itself a commandable
  power device on the bus, or
- **STOR** — an energy-storage power device role, one of the standard's commandable device
  classes.

This was flagged as an open, blocking question in `docs/gap-analysis-meteor-parity.md`'s "TMS data
models" findings: the role choice gates three TMS-required capabilities that otherwise have no
answer —
- **`DeviceInfo`** (MIL-STD-3071 requires it for every device role; the legacy Python prototype
  modeled the whole descriptor family — `ProductInfo`, `PowerPortInfo`, `TopicInfo` — but never
  wired it to a working emitter either),
- **`Reply`** (the standard requires a `Reply` within 0.1 s of any `Request`, for every role — a
  passive MON might reasonably have no command surface to reply *about*, while a power device
  role has an unambiguous obligation to answer), and
- **`Heartbeat`** (the TMS-required 1 s on-bus liveness beacon — distinct from the systemd
  watchdog, which only proves the *process* is alive, not that it's participating on the TMS bus).

The baseline Python prototype never resolved this either — its `DeviceInfo`/`Reply`/`Heartbeat`
models were authored but never wired to a working transformer, so there is no working prior
behavior to default to.

## Decision

**This Rust adapter is a STOR-role power device proxying the battery — not a MON.** Locked in
`crates/bin/rpi/src/config/types/device_config.rs` (`DeviceConfig::role` defaults to the literal
string `"STOR"`) and documented as the authoritative call in `docs/tms-compliance.md` §1. Consequences
of the STOR choice:

- **`DeviceInfo` is mandatory and emitted** — at startup plus every 60 s (not retained; see
  `docs/tms-compliance.md` §3.4.2 for why retain isn't available on the embedded broker's
  in-process link).
- **`Reply` is mandatory and unconditional** — every well-formed `Request` gets `ReplyCode::Ok`
  (echoing `request_id`) within budget; a malformed payload still gets a `Reply`
  (`ReplyCode::InvalidRequest`), never silence. This adapter exposes **no command-kind/payload
  field of its own** — it is a telemetry-only battery proxy that acknowledges liveness, not a
  device that accepts and executes remote commands. That is a direct, deliberate consequence of
  "proxying the battery" rather than being a general commandable power device: the STOR role
  answers, but has nothing to *do* in response to a command yet.
- **`Heartbeat` is mandatory and emitted** every 1 s, independent of poll cadence.

## Consequences

- Three previously-missing TMS topics (`Heartbeat`, `DeviceInfo`, `Reply`) went from "no repo
  hits" to implemented and wired, closing three `high`/`medium`-severity findings from the gap
  analysis in one decision.
- `ReplyCode`'s 6-variant set (`Ok`/`Unknown`/`InvalidRequest`/`Unsupported`/`Timeout`/`Error`) and
  `Request`'s minimal `{device_id, request_id}` shape (no command-kind field) are both best-effort,
  **not IDL-verified** — no `chapter-d.txt`/reference prototype was available when they were
  authored (see `docs/tms-compliance.md` §5 for the full PROVISIONAL inventory). They would need
  revisiting if this adapter is ever asked to accept an actual command, since the current shape
  has nowhere to carry one.
- `Request`/`Reply` is implemented as a loopback `rumqttc` client
  (`bin-rpi::tasks::reply_listener`) rather than the in-process broker `Link`, because wiring a
  live subscriber through `LocalBroker`'s existing `Link` needs a field addition outside this
  workstream's owned-file boundary (see this session's `LinkRx` spike result). Confirmed
  functionally correct (`test-e2e/tests/request_reply_e2e.rs` proves the round trip completes well
  under the 0.1 s budget) but not the eventual "native" in-process path.

## Revisit trigger

- This adapter is ever asked to accept and act on a real command (not just acknowledge liveness) —
  at that point STOR-vs-MON is unaffected, but `Request`'s shape and `ReplyCode`'s semantics need a
  real design pass, not the current placeholder.
- A real MIL-STD-3071 IDL/reference becomes available to verify `ReplyCode`/`Request`/`DeviceInfo`
  field shapes against (see `docs/tms-compliance.md`'s PROVISIONAL inventory for the exact list of
  what would need re-checking).

## Superseded in part (2026-07-16)

The revisit trigger above fired: `docs/compliance/tms/mil-std-3071_data_model.idl` (IDL v2.0.2)
landed as ground truth, and it contradicts two parts of this record's original decision. The
STOR-role choice itself stands (see below) — what's superseded is (a) "role" being a single
adapter-wide constant, and (b) the `Request`/`Reply` shape and posture this record locked in.

**(a) Role is no longer always-STOR.** Decision 0007 (TPM/TEM naming) establishes that this
adapter's per-bus identity derives its MIL-STD-3071 role from the bus protocol, not one
adapter-wide literal:

- The RS-485/Seplos battery bus maps to `ROLE_STORAGE` — this is exactly the `"STOR"` literal this
  record originally locked, now confirmed against the real IDL (`mil-std-3071_data_model.idl` lines
  481-491) rather than the "not IDL-verified" placeholder this record shipped with.
- The RS-232/Voltronic inverter bus (decision 0001, in-scope) maps to `ROLE_CONVERSION` — a role
  this record never considered, because the inverter leg was out of scope when it was authored.

A unit with both legs wired therefore runs two roles concurrently, one per bus, not one role for
the whole adapter. The STOR-vs-MON *choice* this record made for the battery leg is unaffected;
what's superseded is treating STOR as the adapter's only possible role.

**(b) The Reply posture changes from unconditional-Ok-on-a-generic-topic to spec-clean.** This
record's original "Reply is mandatory and unconditional... every well-formed Request gets
`ReplyCode::Ok`" was built on a `Request`/`Reply` shape this record itself flagged as "best-effort,
not IDL-verified... authored when no chapter-d.txt/reference prototype was available." The IDL
proves that placeholder wrong in a specific way, tracked in GitHub issue #257: **the generic
`Request` topic this record assumed does not exist in MIL-STD-3071.** The IDL has no `@topic`
struct named `Request` — `Reply` is real (`@topic`, IDL lines 4391-4426, keyed on
`requestingDeviceId`/`targetDeviceId`, carrying a `ReplyStatus{code, reason}`), but there is no
matching generic inbound `Request` topic for it to answer; real requests are the four **named,
typed** request topics (`GridRequest`, `DeviceRequest`, `DeviceConfigRequest`,
`PowerPortConfigRequest` — IDL lines 1219, 2600, 2575, 3948). This adapter's own invented
`Request{device_id, request_id}` + `topics::REQUEST` were never a real MIL-STD-3071 topic.

The spec-clean posture, replacing the unconditional-Ok behavior above: this adapter answers the
**named** request topics, each with `ReplyCode::REPLY_NOT_IMPLEMENTED` (a real IDL enumerator,
`mil-std-3071_data_model.idl` line 633) rather than a fabricated `Ok`, and
`TopicInfo.supported_request_topics` (IDL: `supportedRequestTopics`, line 4136) is declared
**empty** — this adapter subscribes to none of the four named request topics for real command
handling, consistent with being a telemetry-only proxy that has nothing to *do* in response to a
command (a point this record's original Consequences section already made, just against the wrong
topic shape).

### Consequences of the supersession

- Any code or test built against the old generic `Request`/unconditional-`Ok` shape needs to
  migrate to the named-topics/`REPLY_NOT_IMPLEMENTED`/empty-`supported_request_topics` posture —
  issue #257 tracks this as landing in wave 1.
- `test-e2e/tests/request_reply_e2e.rs` (cited in this record's original Consequences as proving
  the round trip) proved the *old*, now-superseded shape; it needs updating against the real named
  topics, not just a passing re-run.
- This does not reopen STOR-vs-MON for the battery leg — it only corrects the topic/posture layer
  the STOR choice was expressed through, and extends role assignment to be per-bus per decision
  0007.

### Revisit trigger (supersession-specific)

- Wave 1's fix (issue #257) lands and the named-topic/`REPLY_NOT_IMPLEMENTED`/empty-list posture is
  actually implemented — at that point this superseded section should be folded into a fully
  rewritten record rather than carried as an addendum.

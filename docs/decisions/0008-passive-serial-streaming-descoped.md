# 0008 — Passive/unsolicited serial streaming descoped; strict request/response stands

**Status:** Locked (2026-07-16)

## Context

Both serial protocols this repo speaks are poll-based: the RS-485/Seplos battery leg issues a
request frame and reads exactly one response, and the RS-232/Voltronic inverter leg (decision 0001,
in-scope this round) does the same — `QID`/`QPIGS` queries, one ASCII reply each. Neither device
class on either bus pushes telemetry unsolicited.

Landing the inverter leg this round adds one new piece of per-vendor robustness: a
Voltronic-specific frame-validation hook — CRC-16/XMODEM verification on the reply, plus a bounded
continuation-read loop to recover when a prior exchange's leftover RX bytes would otherwise desync
the next `read_until`. GitHub issue #268 (`lib-io: SerialTransport lacks discard_input(); stale
bytes can desync exchanges`) names this precisely: *"the Voltronic continuation-read loop bounds
the damage; a `discard_input()` default method on `SerialTransport` ... would fix it properly.
Evaluate after field captures show whether desync occurs in practice."* That bounded, per-vendor
hook is real, scoped work landing this round — it is not the same thing as passive streaming, and
this record exists to keep the two from blurring together.

A **generic framing-strategy layer** — an abstraction that would let a future transport declare
"I emit unsolicited frames, here's how to delimit them" independent of any specific vendor — was
also raised as a candidate this round and is being explicitly declined here, per CLAUDE.md's
*Abstraction & Platform Seams* guidance: a seam is worth building when a second concrete consumer
needs it, not speculatively ahead of one. Today there is exactly one bounded-continuation-read
consumer (Voltronic); a generalized framing layer built for a still-hypothetical second unsolicited
source would be the same kind of debt CLAUDE.md warns against — an abstraction with no second
implementation to keep it honest.

## Decision

**Serial stays strictly request/response.** The per-vendor frame-validation hook landing this round
(Voltronic CRC verify + the bounded continuation-read loop, `SerialTransport`-seam-consuming per
decision 0001) is the recorded stopping point for this workstream. True passive/unsolicited stream
ingestion, and a generic framing-strategy abstraction to support it, both stay descoped.

## Consequences

- No device class in scope today (Seplos battery, Voltronic inverter) requires anything past
  request/response + a bounded recovery read — the current `SerialTransport` seam shape is
  sufficient and needs no unsolicited-frame concept added to it.
- The bounded continuation-read loop is a stopgap, not a general solution: it recovers from a
  *known, bounded* desync (stale bytes from a prior timed-out exchange), not from an arbitrary
  unsolicited stream. Issue #268's proper fix (`discard_input()` on `SerialTransport`) remains open
  and is the more correct long-term answer to the specific desync case, independent of this
  decision.
- If a future device class does stream unsolicited frames, it gets its own `SerialTransport` impl
  and its own framing decision at that time — this record does not pre-design that seam speculatively.

## Revisit trigger

- A device class is added to scope that pushes unsolicited/passive frames (no query, device
  transmits on its own schedule) — at that point a real framing-strategy layer has a genuine second
  consumer and stops being speculative.
- Field captures on the Voltronic leg show desync the bounded continuation-read loop cannot recover
  from — cross-reference GitHub issue #268; that would force the `discard_input()` fix (or an
  equivalent) regardless of this decision's scope line.

# 0006 — Meshtastic framing: hand-rolled minimal protobuf, `meshtastic` crate as fallback

**Status:** Locked (2026-07-16) — this record locks the *approach*, not its implementation.
**Corrected 2026-07-25** (delight-features re-verification): the approach itself is **confirmed,
not reversed** — hand-rolling is still the right call, and for a sharper reason than originally
argued (see "Correction 2 — the fallback" below). What is corrected is this record's account of
*implementation status*, the *field-number provenance*, the *module path*, and — most importantly
— a **falsified core assumption** that likely made every frame sent before this correction
undeliverable. See ADR [0012](./0012-delight-features-scope-and-seams.md) for the surrounding work
this correction landed alongside, and the "Meshtastic Serial/UART Protobuf Client API (Heltec V4)"
research note (Basic Memory) for the primary-source verification this correction is built on.

## Correction 1 — implementation status, module path, field-number provenance

The original status banner claimed: *"`crates/lib-mesh/src/impl_frame.rs` is a 2-3 line stub
placeholder... no `ToRadioFrame` implementors, no `write_varint`/`write_tag`/`write_len_delimited`
functions, and no golden byte tests exist in the repo yet."* All three claims are now false:

- **The module moved.** `crates/lib-mesh` was folded into `crates/lib-net::mesh` by #287. The
  encoder lives at `crates/lib-net/src/mesh/impl_frame.rs`.
- **The encoder is real and complete**, not a stub: `write_varint`, `write_tag`,
  `write_varint_field`, `write_fixed32_field`, `write_len_delimited_bytes`/`_nested`,
  `encode_data`/`encode_mesh_packet`/`encode_to_radio`, and a `ToRadioFrame` impl for `MeshFrame` —
  388 lines including a `tests` module with ten `#[test]` functions, several of them exact
  golden-byte comparisons (e.g. `golden_bytes_channel_zero_omits_the_channel_field`,
  `golden_bytes_nonzero_channel_is_included`, `golden_bytes_to_field_is_always_broadcast`).
- **The field numbers are no longer PROVISIONAL.** `ToRadio.packet = 1`, `MeshPacket.to = 2`
  (`fixed32`), `MeshPacket.channel = 3`, `MeshPacket.decoded = 4`, `Data.portnum = 1`,
  `Data.payload = 2`, `PortNum.PRIVATE_APP = 256` are all **verified** against
  `meshtastic/protobufs` @ master and cross-checked against `meshtastic/firmware` @ master
  (v2.7.26, 2026-06-24) — this record's own revisit trigger ("the upstream `.proto` sources become
  fetchable... letting the field-number provenance be verified without needing live hardware") is
  met.

## Correction 2 — the fallback (licensing finding)

The original "Consequences" section recorded the documented fallback as: *"swap `impl_frame.rs`
for the real `meshtastic` crate, or `prost` + the pinned upstream `.proto` files."*

**Both `meshtastic/rust` (the `meshtastic` crate, GPL-3.0) and `meshtastic/protobufs` (the upstream
`.proto` sources, GPL-3.0) are copyleft.** Pulling in the `meshtastic` crate directly links
GPL-3.0-licensed code into this proprietary binary. Less obviously, the `prost` alternative doesn't
escape it either: generating from the pinned upstream `.proto` files — even via a permissively
licensed codegen tool (`prost` is Apache-2.0; so is `micropb`) — still means *consuming* GPL-3.0
source to produce the generated types, a copyleft ingestion event either way. **This was never
weighed in the original record**, which reasoned about dependency footprint (hundreds of unused
generated message types) but not licence terms.

**The licence argument for hand-rolling is stronger than the dependency-footprint argument this
record actually made.** If the hand-rolled encoder ever needs replacing, the defensible fallback is
`micropb`/`micropb-gen` (MIT/Apache-2.0, `no_std`, no-alloc, `heapless` containers) generating from
a **locally re-declared minimal `.proto`** — one containing only the four fields this crate sends,
written from the *documented* wire format and field numbers (facts, and published on the Buf
Schema Registry), not copied from the GPL-3.0 upstream source tree.

## Correction 3 — the falsified `to`-field assumption (the most important correction here)

The original record stated: *"`MeshPacket.to` (destination node) is never written (relying on
proto3's absent-field-means-broadcast convention, itself unconfirmed against real firmware
behavior)."*

**This assumption is falsified.** `meshtastic/firmware`'s `src/mesh/Router.cpp`, `sendLocal()`,
contains:

```c
if (p->to == 0) { LOG_ERROR("Packet received with to: of 0!"); }
```

No firmware path rewrites an absent/zero `to` to `NODENUM_BROADCAST`. A frame with no `to` field is
therefore very likely **never delivered** — not a cosmetic gap, but a silent total loss of the
out-of-band mesh channel, which exists specifically to survive a network outage. Every frame this
crate sent before this correction landed was likely dropped by the recipient's own router before
reaching the application layer.

**Fix landed (#386):** `encode_mesh_packet` now always emits `MeshPacket.to = 0xFFFFFFFF`
(broadcast) explicitly — `fixed32` field 2, wire type 5: tag `0x15`, then four `0xFF` bytes. The
golden tests were updated to assert this byte sequence (`15 FF FF FF FF`) is present in every
encoded frame regardless of channel or payload.

## Context (original, unchanged)

`docs/gap-analysis-meteor-parity.md`'s "Meshtastic mesh-radio forwarding" finding flags the
baseline's entire second publish target — LoRa mesh forwarding via a Meshtastic radio, including a
greedy ~200-byte payload-condensation algorithm — as a clean capability drop with zero prior Rust
implementation. Closing it means producing on-air Meshtastic protobuf frames
(`ToRadio{ packet: MeshPacket{ to, channel, decoded: Data{ portnum, payload } } }`) plus the serial
stream framing (`0x94 0xC3` magic + big-endian `u16` length prefix) Meshtastic radios expect.

Two implementation strategies were live candidates:

1. **Pull in a real protobuf toolchain** — either `prost` + the pinned upstream
   `meshtastic/protobufs` `.proto` sources (a build-time codegen step), or the community
   `meshtastic` crate directly (its full generated message graph — hundreds of message types this
   adapter will never send).
2. **Hand-roll the minimal encoder** for exactly the one message shape needed: four scalar/bytes
   fields, protobuf varint + fixed32 + length-delimited wire encoding — a small, stable,
   openly-specified binary format.

## Decision (confirmed, not reversed)

**Hand-roll a minimal protobuf encoder** (`crates/lib-net/src/mesh/impl_frame.rs`) for the single
message shape this crate needs, rather than pulling in `prost` or the `meshtastic` crate. Four free
functions (`write_varint`, `write_tag`, `write_varint_field`/`write_fixed32_field`,
`write_len_delimited_bytes`/`_nested`) build the nested `Data` → `MeshPacket` → `ToRadio` protobuf
bytes by hand, writing backward from the end of the caller's buffer (no scratch buffer, no
allocation — see the module's own doc comment for the reverse-encoding rationale), then prepend the
stream-framing header. This is pinned by byte-level golden tests rather than relying on a codegen'd
struct's `Debug`/round-trip to prove correctness.

## Consequences

- **Dependency footprint stays minimal**: no protobuf codegen toolchain, no hundreds of unused
  generated message types, for four fields — appropriate for a bandwidth- and budget-constrained
  on-air payload where every byte of the encoder's own correctness surface is worth minimizing, not
  maximizing.
- **No copyleft ingestion**: the hand-rolled encoder is written from the documented wire format and
  field numbers, never from GPL-3.0 source. This is now understood to be the *stronger* of the two
  original arguments for hand-rolling (see Correction 2).
- **Correctness rests on golden byte tests**, not a battle-tested codegen'd implementation — the
  hand-rolled encoder is small enough to review byte-for-byte, and it is now pinned by ten tests
  including several exact golden-byte comparisons. It has *not* shipped against real Meshtastic
  hardware yet (see Revisit trigger).
- **If the hand-rolled framing turns out to be wrong on real hardware**, the corrected fallback
  (see Correction 2) is `micropb` generating from a locally re-declared minimal `.proto` — **not**
  the GPL-3.0 `meshtastic` crate or a `prost` build against the GPL-3.0 upstream `.proto` sources.
  This remains a deliberately reversible decision: the module boundary (`ToRadioFrame::to_radio_frame`)
  is narrow enough that swapping the implementation strategy behind it doesn't ripple into
  `crate::condense`/`crate::run_radio`.
- The condensation algorithm's field set, priority order, and JSON key names (`impl_condense.rs`)
  are a separate call from the framing choice this record covers — see that file's own doc comment.

## Revisit trigger

- ~~Real Meshtastic hardware becomes available for field testing and the hand-rolled frame either
  works (record supersedes to "verified") or fails (swap to the `meshtastic` crate/`prost` per the
  documented fallback above).~~ **Still open** — no HIL run against a real Meshtastic radio has
  happened yet. Say "implemented, HIL validation pending," not "verified," until it does.
- ~~The upstream `meshtastic/protobufs` `.proto` sources become fetchable in a future environment,
  letting the field-number provenance be verified without needing live hardware.~~ **Met
  (2026-07-25)** — field numbers verified against `meshtastic/protobufs` @ master; see Correction 1.
- **New trigger added by this correction:** if the fallback is ever exercised, re-verify at that
  time that `micropb`'s licence and no_std/no-alloc posture haven't changed, and that the locally
  re-declared `.proto` is written from documented facts, not copied from GPL-3.0 source.

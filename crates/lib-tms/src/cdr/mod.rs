//! Hand-written, dependency-free XCDR2 serialization core for the TMS DDS transport.
//!
//! This module owns the CDR "machine" — the [`CdrSink`] write abstraction, its two impls
//! ([`CdrWriteSink`] / [`CdrCountingSink`]), the type-level field chain ([`CdrField`] / [`CdrChain`]
//! / [`CdrNode`] / [`CdrNil`]), the DHEADER/optional/encapsulation emit helpers, and a minimal
//! request [`decode`] path. It has NO dependency beyond `core` (no `alloc`, no `serde`), so it links
//! on the `thumbv7em-none-eabihf` MCU target and is reusable there. The GENERATED per-type chains
//! (WP3) plug into this via [`CdrField`]/[`CdrChain`]; the canonical→CDR bindings live in
//! `lib-mapping`.
//!
//! # Why hand-written and oracle-first
//!
//! XCDR2 wire mechanics are subtle and version-dependent, so every rule below is pinned to the
//! normative spec text (DDS-XTypes 1.3, OMG `formal/2020-02-04`; DDSI-RTPS 2.3) with a section cite,
//! and each has a literal hex-vector test in `tests/cdr_primitives.rs` authored BEFORE the code. The
//! byte-oracle chain (WP3) then cross-checks these against a reference serializer and CycloneDDS.
//!
//! # Resolved XCDR2 verify-items (spec-pinned)
//!
//! All 136 TMS IDL types are `@extensibility(APPENDABLE)` with implicit member IDs, so the whole
//! model is uniform **DELIMITED_CDR** (DHEADER-delimited, member-by-member, no PL_CDR/EMHEADER).
//!
//! 1. **DHEADER** — DDS-XTypes 1.3 §7.4.3.4.1 (+ §7.4.2, rule (30) §7.4.3.5.3). An APPENDABLE type
//!    is serialized as `{ DHEADER : UInt32 } << { O : AsFinal(O.type) }`. `DHEADER(O) = O.ssize` =
//!    the number of bytes of the serialized members that follow, **not** including the DHEADER
//!    itself. Being a `UInt32` it forces 4-byte alignment (up to 3 pad bytes may precede it) and
//!    uses the stream endianness. DHEADERs **nest**: a top-level APPENDABLE type gets one (rule (1)
//!    → `AsNested` → rule (30)), and every nested APPENDABLE member/element gets its own.
//!    Implemented by [`emit_dheader`] via bounded dual traversal (count the subtree, write the
//!    length, emit the subtree) — NO back-patching. Sequences/arrays/maps of NON-primitive elements
//!    also carry a DHEADER (rules (9)/(12)/(15)); sequences/arrays of PRIMITIVE elements do NOT
//!    (rules (8)/(11)/(14)).
//!
//! 2. **Encapsulation identifier** — DDS-XTypes 1.3 §7.6.3.1.2 Table 60. On the RTPS wire, an
//!    APPENDABLE type with encoding version 2, little-endian, uses **`D_CDR2_LE = {0x00, 0x09}`**
//!    ([`ENCAPSULATION_D_CDR2_LE`]), followed by a 2-byte **options** field ([`OPTIONS_ZERO`]). The
//!    options' second byte's least-significant two bits encode the count of padding bytes appended
//!    after the payload to reach the next 4-byte boundary (§7.6.3.1.2), set by
//!    [`CdrWriteSink::finalize_encapsulation_padding`]. (`CDR2_LE = {0x00,0x07}` is the FINAL-type
//!    variant; PLAIN/PL variants are unused since no TMS type is FINAL or MUTABLE.) **Surprise:**
//!    the encoding-VM's `ENC_HEADER` notation in §7.4.3.4 Table 39 uses DIFFERENT bytes
//!    (PLAIN_CDR2 `{0x00,0x11}`, DELIMIT_CDR `{0x00,0x15}`) — those are the VM's internal
//!    format tags, NOT the RTPS wire identifier. The bytes that go on the wire, and that CycloneDDS
//!    reads, are the Table 60 values used here.
//!
//! 3. **Optional members in APPENDABLE** — DDS-XTypes 1.3 §7.4.3.5.2 + rule (20) §7.4.3.5.3.
//!    PLAIN_CDR2 / DELIMITED_CDR serialize an optional member as a `boolean <is_present>` (`0x01`
//!    present, `0x00` absent), followed by the member value IFF present. Implemented by the single
//!    [`emit_optional`] helper (and [`emit_optional_seq`] for a sequence-valued optional). **WP5
//!    CycloneDDS arbitration — RESOLVED, NO FLIP** (story #323): the ambiguity below was that for a
//!    *trailing* optional member, APPENDABLE's DHEADER-length truncation *also* lets a reader detect
//!    absence, so some stacks might omit the `<is_present>` boolean entirely for an absent trailing
//!    member — dust_dds 0.15.0 (the WP0 byte-oracle for optional-free topics) in fact cannot encode
//!    "absent" at all (WP0 finding). Empirically arbitrated against **CycloneDDS (eclipse-cyclonedds
//!    Python binding `cyclonedds` 11.0.1)**: its own encoder emits the identical spec-literal form
//!    (`<is_present>` boolean present AND absent, byte-for-byte matching this module's output for
//!    both the minimal `ActiveMicrogridControllerState.master_id` case and the real
//!    `energy_start_stop_state`/`power_hardware_inverter` golden fixtures), and its own decoder
//!    ACCEPTS the exact committed hand-derived vectors in `wire_cdr_cross_oracle.rs`
//!    (`active_microgrid_controller_optional_{present,absent}_hand_derived`) unmodified. This code's
//!    spec-literal form (always emit the boolean) is therefore the interop-correct one; no code
//!    change. Full evidence: `deploy/dds-probe/dump_reference.py`'s `arbitration` mode + the WP5
//!    report. Routing every optional through [`emit_optional`]/[`emit_optional_seq`] kept the
//!    arbitration a one-place flip point even though, in the end, none was needed.
//!
//! 4. **Alignment** — DDS-XTypes 1.3 §7.4.3.2 (Tables 36/37), §7.4.2. XCDR2 max alignment is
//!    **4** (`MAXALIGN(VERSION2) = 4`), so `INT64`/`UINT64`/`FLOAT64` align to 4, not 8. An object
//!    aligns such that `(offset - origin) % min(type_align, 4) == 0`. Rule (1) sets `ORIGIN = 0` at
//!    the encapsulation identifier and keeps it there; since the encapsulation header is exactly 4
//!    bytes, the payload begins at `offset 4 ≡ phase 0`. This code therefore measures alignment from
//!    absolute buffer position (`pos % 4`), which equals the spec's `(offset - origin) % 4`. (The
//!    common shorthand "origin = first byte after the encapsulation header" is equivalent for
//!    mod-4 purposes.) This max-align-4 rule is load-bearing for item (1)'s dual traversal — see
//!    "Position independence" below.
//!
//! 5. **Enums** — DDS-XTypes 1.3 §7.4.3.5.3 rule (5) + §7.4.1.1.3.1 Table 32 + §7.3.1.2.1.5. An
//!    enum serializes as its holder integer; the default `@bit_bound` is **32** ("32 bits is the
//!    default size, and corresponds to all enumerated types prior to this specification"), i.e. a
//!    4-byte value (align 4). The spec holder is `Int32` (signed); [`CdrSink::enum_u32`] emits 4
//!    little-endian bytes, byte-identical for the TMS ordinals (all small, non-negative).
//!
//! 6. **Strings** — DDS-XTypes 1.3 §7.4.3.5.3 rule (3). A `string` is a `UInt32` length prefix that
//!    **includes the NUL terminator**, then the body bytes, then the NUL byte. The length prefix is
//!    4-byte aligned. An empty string is length `1` + a single `0x00`. ([`CdrSink::string`].)
//!
//! 7. **Sequences** — DDS-XTypes 1.3 §7.4.3.5.3 rules (11)/(12). A sequence of PRIMITIVE elements
//!    (rule (11)) is `{ length : UInt32 } << elements*` — length is the element COUNT, no delimiter
//!    ([primitive path in the generated chains]). A sequence of NON-primitive elements (rule (12) —
//!    struct, string, or enum elements) is preceded by a **NEXTINT/DHEADER** `UInt32` giving the byte
//!    length of `{ length + elements }`, i.e. `{ NEXTINT } << { length : UInt32 } << element*`. A
//!    sequence of APPENDABLE structs is therefore
//!    `{ NEXTINT } << { length } << { element(DHEADER + members) }*` ([`emit_appendable_seq`]); a
//!    sequence of strings is `{ NEXTINT } << { length } << { string }*` ([`emit_string_seq`]).
//!    **History (this is the SETTLED reading — WP6.5):** WP2 originally emitted the rule-(12)
//!    delimiter; WP3 REMOVED it because the then-oracle (dust_dds 0.15.0, unpatched) does not emit
//!    it and `chains == reference` was the governing contract. WP7a then proved with STOCK tooling
//!    that **CycloneDDS 11.0.1's decoder REJECTS** rule-(12)-less non-primitive sequences (live
//!    `DcMeasurementUpdate` deserialization failure; primitive-only `Heartbeat` was fine) — so
//!    dust_dds, not CycloneDDS, was the outlier, and CycloneDDS is the interop arbiter per the round
//!    done-bar. The delimiter is therefore RE-ADOPTED here, and dust_dds is vendored + patched to
//!    emit it too (`vendor/dust_dds/PATCHES.md`), restoring `chains == dust_dds(patched) == what
//!    CycloneDDS accepts`. This is a live upstream dispute (eclipse-cyclonedds/cyclonedds-cxx#218;
//!    OMG erratum DDSXTY14-56); CycloneDDS's behavior governs the wire. (Neither request type
//!    contains a sequence; the battery/inverter telemetry topics do.)
//!
//! 8. **KeyHash** — DDSI-RTPS 2.3 §9.6.3.8 (+ XTypes 1.3 §7.6.8). `KeyHash_t` is a 16-byte octet
//!    array computed from the `@key` members serialized in declaration order using **CDR
//!    Big-Endian**. Algorithm choice is by TYPE (max serialized key size), not value: if the max
//!    serialized size of all key fields is **≤ 128 bits (16 bytes)**, the KeyHash is the
//!    big-endian CDR of the key fields, zero-filled to 16 bytes; **otherwise** it is the 128-bit MD5
//!    (RFC 1321) of that big-endian CDR. This module provides only emit-key *plumbing*
//!    ([`emit_key`], which serializes a key-field chain through a sink in declaration order); the
//!    big-endian sink + 16-byte/MD5 wrapping is deferred to WP4 (only if the winning DDS adapter
//!    needs an explicit KeyHash — instance identity is carried by `USER_DATA = deviceId`).
//!
//! # Position independence (why the dual traversal is single-pass-per-DHEADER)
//!
//! [`emit_dheader`] counts a subtree's size with a FRESH [`CdrCountingSink`] at phase 0, then emits.
//! This is correct because a DHEADER-delimited subtree ALWAYS begins at a 4-byte-aligned offset (the
//! DHEADER is a 4-byte `u32`), and XCDR2's max alignment is 4 (item 4) — so the subtree's internal
//! padding depends only on offsets relative to its own 4-aligned start, never on its absolute
//! position. The alignment-phase property test (`tests/cdr_primitives.rs`) pins this: a subtree's
//! content size counted after the mandatory `align(4)+DHEADER` is identical at every enclosing phase
//! 0..4. If a future construct violated it (e.g. if XCDR2 permitted 8-byte alignment), the counting
//! would need to thread the real phase — the test would fail loudly first.

mod prelude;
pub mod types;

mod impl_cdr_counting_sink;
mod impl_cdr_nil;
mod impl_cdr_node;
mod impl_cdr_reader;
mod impl_cdr_write_sink;

pub mod decode;

pub use types::*;

/// RTPS encapsulation identifier for an APPENDABLE type, XCDR2 (encoding version 2), little-endian:
/// `D_CDR2_LE` (DDS-XTypes 1.3 §7.6.3.1.2 Table 60). Every TMS type is APPENDABLE, so this is THE
/// identifier for every TMS payload.
pub const ENCAPSULATION_D_CDR2_LE: [u8; 2] = [0x00, 0x09];

/// RTPS encapsulation identifier for a FINAL type, XCDR2, little-endian: `CDR2_LE` (DDS-XTypes 1.3
/// §7.6.3.1.2 Table 60). Not used to emit any TMS type (none are FINAL), but accepted on decode.
pub const ENCAPSULATION_CDR2_LE: [u8; 2] = [0x00, 0x07];

/// The 2-byte options field written directly after the encapsulation identifier. Initialized to
/// zero; the trailing-padding bits (low two bits of the second byte) are set at finalization by
/// [`CdrWriteSink::finalize_encapsulation_padding`] (DDS-XTypes 1.3 §7.6.3.1.2).
pub const OPTIONS_ZERO: [u8; 2] = [0x00, 0x00];

/// Round `pos` up to the next multiple of `min(n, 4)` — the XCDR2 alignment rule
/// (`neededalign = MIN(N, maxalign)`, `maxalign = 4`; DDS-XTypes 1.3 §7.4.3.2.2). Shared by every
/// [`CdrSink`] impl and [`CdrReader`] so write, count, and read pad identically.
pub(crate) fn align_up(pos: usize, n: usize) -> usize {
    let a = if n > 4 { 4 } else { n };
    if a <= 1 {
        return pos;
    }
    // a is 2 or 4 (a power of two); round up.
    (pos + a - 1) & !(a - 1)
}

/// Emit the 4-byte RTPS encapsulation header for a TMS (APPENDABLE) payload: `D_CDR2_LE` +
/// zeroed options (DDS-XTypes 1.3 §7.6.3.1.2 Table 60). Written with [`CdrSink::bytes`] (no
/// alignment — it is the origin).
#[inline(always)]
pub fn emit_encapsulation<S: CdrSink>(sink: &mut S) {
    sink.bytes(&ENCAPSULATION_D_CDR2_LE);
    sink.bytes(&OPTIONS_ZERO);
}

/// Emit an APPENDABLE type: a DHEADER holding the serialized size of the member chain `C`, then the
/// members (DDS-XTypes 1.3 §7.4.3.5.3 rule (30) + §7.4.3.4.1). Bounded dual traversal, NO
/// back-patching: a fresh [`CdrCountingSink`] (phase 0) counts the subtree, the `u32` DHEADER is
/// written (self-aligning to 4), then the same chain emits the real bytes. Correctness of the
/// phase-0 count is argued in the module doc ("Position independence").
#[inline(always)]
pub fn emit_dheader<Src: ?Sized, C: CdrChain<Src>, S: CdrSink>(src: &Src, sink: &mut S) {
    let mut counter = CdrCountingSink::new();
    C::emit(src, &mut counter);
    let ssize = counter.len();
    sink.u32(ssize as u32);
    C::emit(src, sink);
}

/// Emit an optional member in an APPENDABLE/PLAIN_CDR2 type: a `boolean <is_present>` then, iff
/// present, the value written by `emit_present` (DDS-XTypes 1.3 §7.4.3.5.2 + rule (20)). The SINGLE
/// place the optional-encoding policy lives — WP5 CycloneDDS-arbitrated CONFIRMED correct, no flip
/// (see module doc item 3). `emit_present` receives the borrowed value and the sink (it may itself
/// call [`emit_dheader`] for a nested APPENDABLE value).
#[inline(always)]
pub fn emit_optional<T, S, F>(sink: &mut S, value: Option<&T>, emit_present: F)
where
    S: CdrSink,
    F: FnOnce(&T, &mut S),
{
    match value {
        Some(v) => {
            sink.boolean(true);
            emit_present(v, sink);
        }
        None => sink.boolean(false),
    }
}

/// Presence-flag wrapper for an `@optional` member whose value is a SEQUENCE (which [`emit_optional`]
/// cannot take — its value isn't a single `&T`): a `boolean <is_present>` then, iff present, the
/// sequence written by `emit_present` (DDS-XTypes 1.3 §7.4.3.5.2 + rule (20)). The ONE OTHER place —
/// besides [`emit_optional`] — an optional's `<is_present>` boolean is written, deliberately kept in
/// lockstep with it so the WP5 CycloneDDS-arbitration policy (module doc item 3; CONFIRMED correct,
/// no flip) would have flipped in both with one edit had the verdict gone the other way.
#[inline]
pub fn emit_optional_seq<S: CdrSink, F: FnOnce(&mut S)>(
    sink: &mut S,
    present: bool,
    emit_present: F,
) {
    if present {
        sink.boolean(true);
        emit_present(sink);
    } else {
        sink.boolean(false);
    }
}

/// Emit a complete RTPS SerializedPayload for a top-level TMS topic type `Src` with member chain
/// `C`: the encapsulation header, then (because every TMS type is APPENDABLE) a DHEADER + members
/// (DDS-XTypes 1.3 rule (1) → `AsNested` → rule (30)). The caller finalizes trailing padding via
/// [`CdrWriteSink::finalize_encapsulation_padding`] after emission (the counting sink has no buffer
/// to pad).
#[inline(always)]
pub fn emit_object_cdr<Src: ?Sized, C: CdrChain<Src>, S: CdrSink>(src: &Src, sink: &mut S) {
    emit_encapsulation(sink);
    emit_dheader::<Src, C, S>(src, sink);
}

/// Emit-key PLUMBING for KeyHash construction (DDSI-RTPS 2.3 §9.6.3.8 — see module doc item 8):
/// serialize a `@key`-member chain `C` through `sink` in declaration order, with NO encapsulation
/// header and NO enclosing DHEADER (the KeyHash is over the bare key fields in sequence).
///
/// This is intentionally incomplete: a spec-conformant KeyHash requires a BIG-ENDIAN sink and then
/// either zero-fill-to-16-bytes (max key size ≤ 128 bits) or a 128-bit MD5 of the result. Those are
/// deferred to WP4 and only if the winning DDS adapter needs an explicit KeyHash (instance identity
/// is otherwise carried by `USER_DATA = deviceId`). Provided now so the key-field wiring exists in
/// one place.
#[inline(always)]
pub fn emit_key<Src: ?Sized, C: CdrChain<Src>, S: CdrSink>(src: &Src, sink: &mut S) {
    C::emit(src, sink);
}

/// Emit an XCDR2 sequence of **APPENDABLE-struct** elements (rule (12), non-primitive):
/// `{ NEXTINT : UInt32 } << { length : UInt32 } << { element }*`, where the NEXTINT/DHEADER is the
/// byte length of everything after it (`length` + all elements) and each element is a delimited
/// subtree ([`emit_dheader`], its own DHEADER + members, rule (30)). The NEXTINT is REQUIRED — a
/// CycloneDDS 11.0.1 decoder rejects the sequence without it (WP7a; see module doc item 7); the
/// vendored+patched dust_dds oracle emits the identical shape (`vendor/dust_dds/PATCHES.md`).
/// `iter_fn` is called three times ({count-for-NEXTINT, count, iterate}) and MUST yield the same
/// elements each call, which the generated accessors do (they re-read the borrowed source).
#[inline]
pub fn emit_appendable_seq<S, ElemSrc, EC, I, F>(sink: &mut S, iter_fn: F)
where
    S: CdrSink,
    EC: CdrChain<ElemSrc>,
    I: Iterator<Item = ElemSrc>,
    F: Fn() -> I,
{
    let count = iter_fn().count() as u32;
    // Rule (12) NEXTINT/DHEADER: byte length of `{ count : u32 } << { element }*`, counted at
    // phase 0 with a fresh sink (the delimiter is a 4-byte `u32`, so the counted subtree begins
    // 4-aligned exactly as it does in the live stream — position-independent, same argument as
    // `emit_dheader`). See module doc item 7.
    let mut counter = CdrCountingSink::new();
    counter.u32(count);
    for item in iter_fn() {
        emit_dheader::<ElemSrc, EC, CdrCountingSink>(&item, &mut counter);
    }
    sink.u32(counter.len() as u32);
    sink.u32(count);
    for item in iter_fn() {
        emit_dheader::<ElemSrc, EC, S>(&item, sink);
    }
}

/// Emit an XCDR2 sequence of `string` elements: `{ length : UInt32 } << { string }*`. Like
/// [`emit_appendable_seq`], there is **NO sequence-level DHEADER** — each element is a
/// self-delimiting length-prefixed string ([`CdrSink::string`], length-includes-NUL). Matches the
/// reference serializer's collection encoding (a sequence never carries its own DHEADER; only its
/// elements self-delimit).
#[inline]
pub fn emit_string_seq<S: CdrSink, T: AsRef<str>>(sink: &mut S, items: &[T]) {
    // Rule (12) NEXTINT/DHEADER: byte length of `{ count : u32 } << { string }*` (see
    // [`emit_appendable_seq`] and module doc item 7).
    let mut counter = CdrCountingSink::new();
    counter.u32(items.len() as u32);
    for s in items {
        counter.string(s.as_ref());
    }
    sink.u32(counter.len() as u32);
    sink.u32(items.len() as u32);
    for s in items {
        sink.string(s.as_ref());
    }
}

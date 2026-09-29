//! [`CdrSink`] — the write-target abstraction the XCDR2 (DDS-XTypes 1.3) serializer writes through.
//!
//! Mirrors the JSON-wire [`crate::fields::JsonSink`] design: ONE trait, two impls
//! ([`super::CdrWriteSink`] emits bytes into a caller buffer, [`super::CdrCountingSink`] tallies the
//! byte count without writing) driven by the *same* call sequence, so a length computed by the
//! counting sink can never desync from bytes emitted by the write sink (the invariant
//! [`crate::cdr::emit_dheader`]'s no-back-patching dual traversal depends on).
//!
//! Every typed method is **self-aligning**: it first advances to the type's XCDR2 alignment
//! (DDS-XTypes 1.3 §7.4.3.5 rule (2): `ALIGN(O.ssize)` before each primitive), capped at the XCDR2
//! maximum alignment of 4 (§7.4.3.2.2 `MAXALIGN(VERSION2) = 4`), then appends the value
//! little-endian (the byte order of the D_CDR2_LE encapsulation, §7.6.3.1.2 Table 60). Alignment is
//! measured from the stream origin; see [`crate::cdr`]'s module doc for why absolute buffer position
//! equals the phase here (origin 0 + a 4-byte encapsulation header ⇒ payload begins at phase 0).

/// A write target for the XCDR2 wire serializer. Each method appends the bytes (or, for the
/// counting impl, the byte *count*) its name says, self-aligning per the XCDR2 rules first.
pub trait CdrSink {
    /// Current offset into the stream, in bytes from the start of the buffer. Because the stream
    /// origin is 0 and the RTPS encapsulation header is exactly 4 bytes, `pos() % 4` is the XCDR2
    /// alignment phase relative to the origin (DDS-XTypes 1.3 §7.4.3.2.1 `offset`/`origin`).
    fn pos(&self) -> usize;

    /// Advance the stream by inserting zero padding bytes until `pos()` is a multiple of
    /// `min(n, 4)` (DDS-XTypes 1.3 §7.4.3.2.2 `ALIGN(N)`: `neededalign = MIN(N, XCDR.maxalign)`,
    /// `maxalign = 4` for encoding version 2). `n` is the requested alignment (1/2/4/8); passing 8
    /// aligns to 4, matching the XCDR2 4-byte cap for `INT64`/`UINT64`/`FLOAT64`.
    fn align(&mut self, n: usize);

    /// Append raw bytes verbatim, with NO alignment (used for the encapsulation header, string
    /// bodies, and the trailing NUL — each of which sits at a position the caller has already
    /// aligned).
    fn bytes(&mut self, b: &[u8]);

    /// Append a `u8` / `octet` (align 1).
    fn u8(&mut self, v: u8);
    /// Append a `u16` little-endian (align 2).
    fn u16(&mut self, v: u16);
    /// Append a `u32` / `unsigned long` little-endian (align 4). Also the wire form of a DHEADER and
    /// of a sequence/string length prefix.
    fn u32(&mut self, v: u32);
    /// Append a `u64` / `unsigned long long` little-endian (align **4** in XCDR2, not 8 —
    /// DDS-XTypes 1.3 §7.4.2: "INT64, UINT64, FLOAT64 … aligned to 4 rather than 8").
    fn u64(&mut self, v: u64);

    /// Append an `i16` / `short` little-endian (align 2).
    fn i16(&mut self, v: i16);
    /// Append an `i32` / `long` little-endian (align 4).
    fn i32(&mut self, v: i32);

    /// Append an `f32` / `float` little-endian (align 4).
    fn f32(&mut self, v: f32);
    /// Append an `f64` / `double` little-endian (align **4** in XCDR2 — see [`CdrSink::u64`]).
    fn f64(&mut self, v: f64);

    /// Append a `boolean`: one byte, `0x01` if true else `0x00` (align 1). Also used for the
    /// optional-member `<is_present>` marker (DDS-XTypes 1.3 §7.4.3.5.2 / rule (20)).
    fn boolean(&mut self, v: bool);

    /// Append a `string`: a `u32` length prefix (align 4) that **includes the NUL terminator**,
    /// then the UTF-8/single-byte body, then the trailing NUL byte (DDS-XTypes 1.3 §7.4.3.5 rule
    /// (3): `{ O.ssize : UInt32 } // includes NUL`, `{ O[i] : Byte }* // includes NUL`). An empty
    /// string serializes as length `1` followed by a single `0x00`.
    fn string(&mut self, s: &str);

    /// Append an enumerated value as its holder integer. The default `@bit_bound` for an enum is 32
    /// (DDS-XTypes 1.3 §7.3.1.2.1.5 / Table 32 "32 bits is the default size"), so the holder is a
    /// 4-byte value (align 4). The spec holder is `Int32` (signed); for the TMS ordinals (all small
    /// non-negative) the little-endian bytes are identical whether emitted as `i32` or `u32`.
    fn enum_u32(&mut self, v: u32);
}

//! XCDR2 (DDS-XTypes 1.3) PRIMITIVE byte-oracle tests — the spec pins for `lib_tms::cdr`.
//!
//! Each of the 8 verify-items from the DDS plan is pinned here to a LITERAL hex vector derived by
//! hand from the normative spec text (section cites in `lib_tms::cdr`'s module doc), authored as the
//! oracle the implementation must satisfy. Plus: the alignment-phase position-independence property
//! (the claim `emit_dheader`'s single-pass-per-DHEADER counting depends on) and counting==emit
//! consistency.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::all)]

use lib_tms::cdr::{
    emit_dheader, emit_encapsulation, emit_optional, CdrChain, CdrCountingSink, CdrField, CdrNil,
    CdrNode, CdrSink, CdrWriteSink, ENCAPSULATION_D_CDR2_LE, OPTIONS_ZERO,
};

/// Helper: run `f` against a fresh `CdrWriteSink` over a 256-byte buffer and return the written
/// bytes as a `Vec` for comparison. Asserts no overflow.
fn write<F: FnOnce(&mut CdrWriteSink)>(f: F) -> Vec<u8> {
    let mut buf = [0u8; 256];
    let mut sink = CdrWriteSink::new(&mut buf);
    f(&mut sink);
    assert!(!sink.overflowed(), "unexpected overflow");
    let n = sink.len();
    buf[..n].to_vec()
}

// ------------------------------------------------------------------------------------------------
// Test field markers + chains (mirrors what the WP3 codegen will emit) — exercise CdrField /
// CdrChain / CdrNode / CdrNil.
// ------------------------------------------------------------------------------------------------

/// A one-`u32` struct: the classic align-4-leading case used to stress the phase property.
struct OneU32 {
    a: u32,
}
struct FieldA;
impl CdrField<OneU32> for FieldA {
    const ORDER: u16 = 0;
    fn emit<S: CdrSink>(src: &OneU32, sink: &mut S) {
        sink.u32(src.a);
    }
}
type OneU32Chain = CdrNode<FieldA, CdrNil>;

// ================================================================================================
// Item 1 — DHEADER = UInt32 byte-length of serialized members (DDS-XTypes 1.3 §7.4.3.4.1 + rule 30)
// ================================================================================================

#[test]
fn item1_dheader_is_ulong_length_of_members() {
    // A one-u32 struct starting at phase 0: DHEADER (align 4, no pad) = 4, then the value.
    let src = OneU32 { a: 0x1122_3344 };
    let bytes = write(|s| emit_dheader::<OneU32, OneU32Chain, _>(&src, s));
    // DHEADER value 0x00000004, then 0x11223344 little-endian.
    assert_eq!(bytes, [0x04, 0x00, 0x00, 0x00, 0x44, 0x33, 0x22, 0x11]);

    // The DHEADER value equals exactly the bytes that follow it.
    let declared = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
    assert_eq!(declared, bytes.len() - 4);
}

#[test]
fn item1_dheader_nesting_counts_inner_dheaders() {
    // A struct whose sole member is itself an APPENDABLE struct: the outer DHEADER's length INCLUDES
    // the inner DHEADER (nesting). Outer contents = inner (DHEADER 4 + u32 4) = 8.
    struct Outer {
        inner: OneU32,
    }
    struct OuterField;
    impl CdrField<Outer> for OuterField {
        const ORDER: u16 = 0;
        fn emit<S: CdrSink>(src: &Outer, sink: &mut S) {
            emit_dheader::<OneU32, OneU32Chain, S>(&src.inner, sink);
        }
    }
    type OuterChain = CdrNode<OuterField, CdrNil>;

    let src = Outer {
        inner: OneU32 { a: 0x00000009 },
    };
    let bytes = write(|s| emit_dheader::<Outer, OuterChain, _>(&src, s));
    assert_eq!(
        bytes,
        [
            0x08, 0x00, 0x00, 0x00, // outer DHEADER = 8 (inner DHEADER 4 + inner u32 4)
            0x04, 0x00, 0x00, 0x00, // inner DHEADER = 4
            0x09, 0x00, 0x00, 0x00, // inner u32 = 9
        ]
    );
}

// ================================================================================================
// Item 2 — Encapsulation identifier D_CDR2_LE {0x00,0x09} + options (DDS-XTypes 1.3 §7.6.3.1.2 T60)
// ================================================================================================

#[test]
fn item2_encapsulation_d_cdr2_le_bytes() {
    assert_eq!(ENCAPSULATION_D_CDR2_LE, [0x00, 0x09]);
    assert_eq!(OPTIONS_ZERO, [0x00, 0x00]);
    let bytes = write(|s| emit_encapsulation(s));
    assert_eq!(bytes, [0x00, 0x09, 0x00, 0x00]);
}

#[test]
fn item2_options_padding_bits_and_trailing_pad() {
    // Encapsulation + a 1-byte payload → payload_len 1 → 3 trailing pad bytes, options low bits = 3.
    let mut buf = [0u8; 16];
    let mut sink = CdrWriteSink::new(&mut buf);
    emit_encapsulation(&mut sink);
    sink.u8(0xAB); // 1-byte payload
    sink.finalize_encapsulation_padding();
    let n = sink.len();
    assert_eq!(
        buf[..n],
        [0x00, 0x09, 0x00, 0x03, 0xAB, 0x00, 0x00, 0x00] // options[1]=3, 3 trailing pad bytes
    );
}

// ================================================================================================
// Item 3 — Optional member in APPENDABLE = boolean <is_present> + value iff present (§7.4.3.5.2 / 20)
// ================================================================================================

#[test]
fn item3_optional_absent_is_single_false_byte() {
    let bytes = write(|s| emit_optional::<u32, _, _>(s, None, |v, s| s.u32(*v)));
    assert_eq!(bytes, [0x00]);
}

#[test]
fn item3_optional_present_is_true_byte_then_aligned_value() {
    let v = 0x0A0B_0C0Du32;
    let bytes = write(|s| emit_optional::<u32, _, _>(s, Some(&v), |v, s| s.u32(*v)));
    // 0x01 present, then align(4) inserts 3 pad bytes before the u32.
    assert_eq!(bytes, [0x01, 0x00, 0x00, 0x00, 0x0D, 0x0C, 0x0B, 0x0A]);
}

// ================================================================================================
// Item 4 — Alignment max 4 in XCDR2 (u64/f64 align to 4, not 8); origin/phase (DDS-XTypes §7.4.2/3)
// ================================================================================================

#[test]
fn item4_u64_aligns_to_four_not_eight() {
    // u8 then u64: only 3 pad bytes (align to 4), NOT 7 (which align-to-8 would give).
    let bytes = write(|s| {
        s.u8(0xAA);
        s.u64(0x1122_3344_5566_7788);
    });
    assert_eq!(
        bytes,
        [
            0xAA, 0x00, 0x00, 0x00, // u8 + 3 pad (align to 4)
            0x88, 0x77, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11, // u64 LE
        ]
    );
}

#[test]
fn item4_f64_aligns_to_four() {
    let bytes = write(|s| {
        s.u8(0x01);
        s.f64(1.0);
    });
    // 1.0f64 = 0x3FF0000000000000 → LE. align 4 ⇒ 3 pad after the u8.
    assert_eq!(
        bytes,
        [0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xF0, 0x3F]
    );
}

#[test]
fn item4_origin_is_phase_zero_after_encapsulation() {
    // After the 4-byte encapsulation header, a u32 needs NO padding (payload begins at phase 0).
    let bytes = write(|s| {
        emit_encapsulation(s);
        s.u32(0x04030201);
    });
    assert_eq!(bytes, [0x00, 0x09, 0x00, 0x00, 0x01, 0x02, 0x03, 0x04]);
}

// ================================================================================================
// Item 5 — Enum default @bit_bound(32) → 4-byte holder (DDS-XTypes 1.3 §7.4.1.1.3 Table 32)
// ================================================================================================

#[test]
fn item5_enum_is_four_byte_aligned_value() {
    let bytes = write(|s| s.enum_u32(7));
    assert_eq!(bytes, [0x07, 0x00, 0x00, 0x00]);

    // After a 1-byte value it aligns to 4 and occupies 4 bytes.
    let bytes = write(|s| {
        s.u8(0x01);
        s.enum_u32(2);
    });
    assert_eq!(bytes, [0x01, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00]);
}

// ================================================================================================
// Item 6 — String = UInt32 length INCLUDING NUL + bytes + NUL (DDS-XTypes 1.3 §7.4.3.5 rule 3)
// ================================================================================================

#[test]
fn item6_string_length_includes_nul() {
    let bytes = write(|s| s.string("AB"));
    // length = 3 (2 body + NUL), then "AB", then NUL.
    assert_eq!(bytes, [0x03, 0x00, 0x00, 0x00, 0x41, 0x42, 0x00]);
}

#[test]
fn item6_empty_string_is_length_one_then_nul() {
    let bytes = write(|s| s.string(""));
    assert_eq!(bytes, [0x01, 0x00, 0x00, 0x00, 0x00]);
}

// ================================================================================================
// Item 7 — Sequences: primitive-element = count+elems (no DHEADER); non-primitive = DHEADER+count+
//          elems, EACH appendable element carrying its own DHEADER (DDS-XTypes 1.3 rules 11/12/30)
// ================================================================================================

#[test]
fn item7_primitive_sequence_is_count_then_elements_no_dheader() {
    // seq<uint32> {0x0A, 0x0B} — rule (11): NO sequence DHEADER, just count + elements.
    let elems = [0x0Au32, 0x0Bu32];
    let bytes = write(|s| {
        s.u32(elems.len() as u32);
        for e in elems {
            s.u32(e);
        }
    });
    assert_eq!(
        bytes,
        [0x02, 0x00, 0x00, 0x00, 0x0A, 0x00, 0x00, 0x00, 0x0B, 0x00, 0x00, 0x00]
    );
}

#[test]
fn item7_appendable_sequence_has_dheader_and_per_element_dheaders() {
    // seq<Struct{ u8 x }> with one element {0x2A} — rule (12): sequence DHEADER + count + element,
    // and the element (APPENDABLE) carries its OWN DHEADER (rule 30).
    struct Elem {
        x: u8,
    }
    struct ElemField;
    impl CdrField<Elem> for ElemField {
        const ORDER: u16 = 0;
        fn emit<S: CdrSink>(src: &Elem, sink: &mut S) {
            sink.u8(src.x);
        }
    }
    type ElemChain = CdrNode<ElemField, CdrNil>;

    let elem = Elem { x: 0x2A };
    // Count the sequence body (count + element) to size the sequence DHEADER.
    let mut counter = CdrCountingSink::new();
    counter.u32(1); // count
    emit_dheader::<Elem, ElemChain, _>(&elem, &mut counter);
    let body_len = counter.len();

    let bytes = write(|s| {
        s.u32(body_len as u32); // sequence DHEADER
        s.u32(1); // element count
        emit_dheader::<Elem, ElemChain, _>(&elem, s); // element with its own DHEADER
    });
    assert_eq!(
        bytes,
        [
            0x09, 0x00, 0x00, 0x00, // sequence DHEADER = 9 (count 4 + element 5)
            0x01, 0x00, 0x00, 0x00, // element count = 1
            0x01, 0x00, 0x00, 0x00, // element's OWN DHEADER = 1
            0x2A, // element u8
        ]
    );
}

// ================================================================================================
// Item 8 — KeyHash rule is documented in lib_tms::cdr's module doc (DDSI-RTPS 2.3 §9.6.3.8);
//          WP2 provides only emit-key plumbing. Here we pin that the plumbing serializes the key
//          chain in declaration order (the big-endian + 16-byte/MD5 wrapping is WP4).
// ================================================================================================

#[test]
fn item8_emit_key_serializes_key_chain_in_order() {
    // Two-key struct {u32 id, string name}; emit_key writes the bare key fields (no encap, no
    // enclosing DHEADER). (LE here — a real KeyHash uses big-endian; see module doc item 8.)
    struct Keyed {
        id: u32,
        name: &'static str,
    }
    struct KId;
    impl CdrField<Keyed> for KId {
        const ORDER: u16 = 0;
        fn emit<S: CdrSink>(src: &Keyed, sink: &mut S) {
            sink.u32(src.id);
        }
    }
    struct KName;
    impl CdrField<Keyed> for KName {
        const ORDER: u16 = 1;
        fn emit<S: CdrSink>(src: &Keyed, sink: &mut S) {
            sink.string(src.name);
        }
    }
    type KeyChain = CdrNode<KId, CdrNode<KName, CdrNil>>;

    let k = Keyed {
        id: 0x20,
        name: "hi",
    };
    let bytes = write(|s| lib_tms::cdr::emit_key::<Keyed, KeyChain, _>(&k, s));
    // id (LE), then string "hi": len=3 (incl NUL), "hi", NUL.
    assert_eq!(
        bytes,
        [0x20, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x68, 0x69, 0x00]
    );
}

// ================================================================================================
// Alignment-phase POSITION-INDEPENDENCE property (order-of-work step 3) — the claim emit_dheader's
// phase-0 counting depends on.
// ================================================================================================

/// The serialized size of a subtree's CONTENTS, measured after the mandatory `align(4) + DHEADER`
/// that precedes an APPENDABLE subtree, when the enclosing stream sits at alignment phase `p`.
fn contents_size_at_phase(p: usize) -> usize {
    let src = OneU32 { a: 0xDEAD_BEEF };
    let mut c = CdrCountingSink::at_phase(p);
    c.align(4); // enclosing alignment before the DHEADER
    c.u32(0); // the 4-byte DHEADER
    let before = c.len();
    <OneU32Chain as CdrChain<OneU32>>::emit(&src, &mut c);
    c.len() - before
}

#[test]
fn property_appendable_subtree_size_is_phase_independent() {
    // THE property: because a DHEADER is 4 bytes and 4-aligned, and XCDR2's max alignment is 4, an
    // APPENDABLE subtree's contents always begin at phase 0 — so its size is identical at every
    // enclosing phase. This is what lets emit_dheader count from a fresh phase-0 sink.
    let base = contents_size_at_phase(0);
    for p in 1..4 {
        assert_eq!(
            contents_size_at_phase(p),
            base,
            "subtree size differs at phase {p} — design would need phase-threaded counting"
        );
    }
}

#[test]
fn property_raw_chain_size_would_vary_without_the_dheader_realignment() {
    // Contrast (documents WHY the DHEADER re-alignment is load-bearing): the SAME u32-leading chain,
    // counted from a raw phase p WITHOUT the align(4)+DHEADER, is NOT phase-independent — phase 1
    // pays 3 alignment-pad bytes that phase 0 does not. The 4-aligned DHEADER is what erases this.
    fn raw_size_at_phase(p: usize) -> usize {
        let src = OneU32 { a: 0 };
        let mut c = CdrCountingSink::at_phase(p);
        let before = c.len();
        <OneU32Chain as CdrChain<OneU32>>::emit(&src, &mut c);
        c.len() - before
    }
    assert_eq!(raw_size_at_phase(0), 4);
    assert_eq!(raw_size_at_phase(1), 7); // 3 pad + 4
}

// ================================================================================================
// Counting == emit consistency (the invariant the no-back-patching DHEADER traversal relies on)
// ================================================================================================

#[test]
fn counting_sink_matches_write_sink_length() {
    let src = OneU32 { a: 0x12345678 };
    let mut c = CdrCountingSink::new();
    emit_dheader::<OneU32, OneU32Chain, _>(&src, &mut c);
    let counted = c.len();

    let bytes = write(|s| emit_dheader::<OneU32, OneU32Chain, _>(&src, s));
    assert_eq!(bytes.len(), counted);
}

//! PROOF — `lib_tms::fields::emit_str_value`'s JSON string escaping is byte-identical to
//! `serde_json_core`'s `&str` serialization: the on-device JSON encoder's `Serializer::push_char`
//! (`serde-json-core-0.6.0/src/ser/mod.rs:99-157`), which `emit_str_value`'s escape table and
//! control-byte handling mirror exactly (see `lib_tms::fields`'s module doc comment).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use lib_tms::fields::JsonSink;

/// A `JsonSink` that only implements `str` (the only method `emit_str_value` calls) — enough to
/// capture the escaped output as an owned `String` for comparison against the oracle.
#[derive(Default)]
struct Collector(String);

impl JsonSink for Collector {
    fn str(&mut self, s: &str) {
        self.0.push_str(s);
    }
    fn raw_int(&mut self, _v: i64) {
        unreachable!("emit_str_value never calls raw_int")
    }
    fn fixed(&mut self, _scaled: i64, _frac_digits: u8, _invert_sign: bool) {
        unreachable!("emit_str_value never calls fixed")
    }
    fn null(&mut self) {
        unreachable!("emit_str_value never calls null")
    }
    fn f32(&mut self, _v: f32) {
        unreachable!("emit_str_value never calls f32")
    }
    fn f64(&mut self, _v: f64) {
        unreachable!("emit_str_value never calls f64")
    }
}

/// The oracle: `serde_json_core::to_slice` of a bare `&str`, which serializes to the same quoted +
/// escaped form `emit_str_value` produces (no surrounding object/array).
fn oracle(s: &str) -> String {
    let mut buf = [0u8; 512];
    let len = serde_json_core::to_slice(&s, &mut buf).expect("encode");
    core::str::from_utf8(&buf[..len]).expect("utf8").to_string()
}

fn check(s: &str) {
    let mut c = Collector::default();
    lib_tms::fields::emit_str_value(&mut c, s);
    assert_eq!(c.0, oracle(s), "mismatch for {s:?}");
}

#[test]
fn plain() {
    check("hello");
    check("");
}

#[test]
fn embedded_quote() {
    check(r#"foo"bar"#);
}

#[test]
fn backslash() {
    check(r"foo\bar");
}

#[test]
fn newline_and_tab() {
    check("a\nb\tc");
}

#[test]
fn control_chars() {
    // Every control byte individually, plus the named two-char escapes (\b \t \n \f \r) and a
    // \u00XX-form sample mixed into running text.
    for b in 0u8..=0x1F {
        check(&String::from(b as char));
    }
    check(" \u{0008} \u{0009} \u{000A} \u{000C} \u{000D} ");
    check(" \u{0000} \u{0001} \u{0007} \u{000E} \u{001F} ");
}

#[test]
fn unicode_multi_byte() {
    check("ä"); // 2-byte
    check("ℝ"); // 3-byte
    check("💣"); // 4-byte
    check("৬"); // 3-byte
    check("hello 💣 world"); // mixed with a safe run on both sides
}

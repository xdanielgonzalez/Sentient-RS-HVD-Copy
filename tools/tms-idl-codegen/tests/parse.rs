//! Parser + resolver unit tests — one fixture per IDL construct, plus the full-IDL parse with
//! top-level count assertions.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use tms_idl_codegen::model::{RFieldType, RKind};
use tms_idl_codegen::overrides::Overrides;
use tms_idl_codegen::{parser, resolve};

fn resolve_snippet(src: &str) -> tms_idl_codegen::model::Model {
    let pp = parser::preprocess(src);
    let topic_map = resolve::scan_topic_map(&pp.source);
    let qos_map = resolve::scan_qos_map(&pp.source);
    let idl = parser::parse_idl(&pp.source).expect("parse");
    resolve::resolve(&idl, &pp.defines, &Overrides::default(), &topic_map, &qos_map)
        .expect("resolve")
}

#[test]
fn parses_nested_modules_and_struct() {
    let m = resolve_snippet(
        r#"
        module tms {
            module dc {
                @nested @extensibility(APPENDABLE)
                struct Foo {
                    float voltage;
                    tms::Identity id;
                };
            };
            typedef string <32> Identity;
        };
        "#,
    );
    let foo = m.find("Foo").unwrap();
    assert_eq!(foo.module, "tms::dc");
    assert_eq!(foo.kind, RKind::Struct);
    assert_eq!(foo.fields[0].name, "voltage");
    assert_eq!(foo.fields[0].ty, RFieldType::F32);
    // typedef string<32> resolved to a bounded string
    assert_eq!(foo.fields[1].ty, RFieldType::Str(32));
}

#[test]
fn parses_enum_with_ordinals() {
    let m = resolve_snippet(
        r#"
        module tms {
            @extensibility(APPENDABLE)
            enum CableSenseStatus {
                CS_UNKNOWN, // a comment
                CS_DISCONNECTED,
                CS_CONNECTED // trailing
            };
        };
        "#,
    );
    let e = m.find("CableSenseStatus").unwrap();
    assert_eq!(e.kind, RKind::Enum);
    assert_eq!(e.variants.len(), 3);
    assert_eq!(e.variants[0].wire_name, "CS_UNKNOWN");
    assert_eq!(e.variants[0].ordinal, 0);
    assert_eq!(e.variants[2].wire_name, "CS_CONNECTED");
    assert_eq!(e.variants[2].ordinal, 2);
    assert_eq!(e.variants[0].rust_name, "Unknown"); // prefix-stripped, advisory
}

#[test]
fn resolves_forward_ref_declaration_order_independent() {
    // `Bar` is used by `Foo` before it is declared.
    let m = resolve_snippet(
        r#"
        module tms {
            @extensibility(APPENDABLE)
            struct Foo { Bar b; };
            @extensibility(APPENDABLE)
            struct Bar { uint32 x; };
        };
        "#,
    );
    assert_eq!(
        m.find("Foo").unwrap().fields[0].ty,
        RFieldType::Ref("Bar".into())
    );
}

#[test]
fn parses_sequence_typedef_and_bound_from_const() {
    let m = resolve_snippet(
        r#"
        module tms {
            const uint32 Seq_MAXLEN = 7;
            @extensibility(APPENDABLE)
            struct Elem { uint16 v; };
            typedef sequence<Elem, Seq_MAXLEN> Seq;
            @extensibility(APPENDABLE)
            struct Holder { Seq items; };
        };
        "#,
    );
    let holder = m.find("Holder").unwrap();
    match &holder.fields[0].ty {
        RFieldType::Seq(elem, n) => {
            assert_eq!(*n, 7);
            assert_eq!(**elem, RFieldType::Ref("Elem".into()));
        }
        other => panic!("expected Seq, got {other:?}"),
    }
    assert_eq!(holder.fields[0].bound, Some(7));
}

#[test]
fn char_array_typedef_becomes_bounded_string() {
    let m = resolve_snippet(
        r#"
        module tms {
            const uint32 Gtin_LEN = 14;
            typedef char Gtin[Gtin_LEN];
            @extensibility(APPENDABLE)
            struct P { Gtin gtin; };
        };
        "#,
    );
    assert_eq!(m.find("P").unwrap().fields[0].ty, RFieldType::Str(14));
}

#[test]
fn member_annotations_key_optional_unit_range() {
    let m = resolve_snippet(
        r#"
        module tms {
            typedef int32 PowerPortNumber;
            @extensibility(APPENDABLE)
            struct M {
                @key tms::Identity deviceId;
                @optional @range(min=0, max=1) @unit("p.u.") float soc;
                @optional PowerPortNumber portNumber;
            };
            typedef string <32> Identity;
        };
        "#,
    );
    let mm = m.find("M").unwrap();
    assert!(mm.fields[0].is_key);
    assert_eq!(mm.fields[0].name, "device_id");
    assert!(mm.fields[1].optional);
    assert_eq!(mm.fields[1].unit.as_deref(), Some("p.u."));
    assert_eq!(mm.fields[1].range, Some((0.0, 1.0)));
    assert!(mm.fields[2].optional);
    assert_eq!(mm.fields[2].ty, RFieldType::I32);
}

#[test]
fn define_used_in_range_annotation() {
    let m = resolve_snippet(
        r#"
        #define RADIAN_MAX 3.1416
        module tms {
            @extensibility(APPENDABLE)
            struct A { @range(min=-RADIAN_MAX, max=RADIAN_MAX) float phase; };
        };
        "#,
    );
    let (lo, hi) = m.find("A").unwrap().fields[0].range.unwrap();
    assert!((lo - -3.1416).abs() < 1e-4);
    assert!((hi - 3.1416).abs() < 1e-4);
}

#[test]
fn override_narrows_bound() {
    let src = r#"
        module tms {
            const uint32 MAX_PORTS = 64;
            @extensibility(APPENDABLE)
            struct Elem { uint16 v; };
            typedef sequence<Elem, MAX_PORTS> PortSeq;
            @extensibility(APPENDABLE)
            struct Holder { PortSeq ports; };
        };
    "#;
    let pp = parser::preprocess(src);
    let topic_map = resolve::scan_topic_map(&pp.source);
    let qos_map = resolve::scan_qos_map(&pp.source);
    let idl = parser::parse_idl(&pp.source).unwrap();
    let mut ov = Overrides::default();
    ov.bounds.insert("MAX_PORTS".into(), 4);
    let m = resolve::resolve(&idl, &pp.defines, &ov, &topic_map, &qos_map).unwrap();
    assert_eq!(m.find("Holder").unwrap().fields[0].bound, Some(4));
}

#[test]
fn full_idl_parses_clean_with_expected_counts() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let overrides = root.join("tools/tms-idl-codegen/tms-overrides.toml");
    let gen = tms_idl_codegen::generate(&root, &overrides).expect("full IDL parses clean");
    // Counts asserted against `grep -c` of the IDL (103 struct, 33 enum).
    assert_eq!(gen.struct_count, 103, "struct count");
    assert_eq!(gen.enum_count, 33, "enum count");
    assert_eq!(gen.model.types.len(), 136);
    // Multi-topic association resolved from the topic module doc comments.
    let dc = gen
        .model
        .types
        .iter()
        .find(|t| t.name == "MeasurementUpdate" && t.module == "tms::dc")
        .unwrap();
    assert!(dc.topics.contains(&"DcMeasurementUpdate".to_string()));
}

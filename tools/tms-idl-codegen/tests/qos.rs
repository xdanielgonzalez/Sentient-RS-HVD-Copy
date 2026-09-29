//! QoS-lift + extensibility-pin tests: `scan_qos_map`'s doc-comment lift, the APPENDABLE-only hard
//! error, D.5 profile-name/presence validation, and the emitted `generated_qos.rs` surface.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashSet;
use std::path::PathBuf;

use tms_idl_codegen::overrides::Overrides;
use tms_idl_codegen::{parser, resolve};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn overrides_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tms-overrides.toml")
}

/// `resolve()`'s 5-arg call, built from a raw snippet (mirrors `parse.rs::resolve_snippet` but
/// returns the `Result` instead of unwrapping, so the hard-error tests can inspect it).
fn resolve_snippet_result(src: &str) -> Result<tms_idl_codegen::model::Model, String> {
    let pp = parser::preprocess(src);
    let topic_map = resolve::scan_topic_map(&pp.source);
    let qos_map = resolve::scan_qos_map(&pp.source);
    let idl = parser::parse_idl(&pp.source).expect("parse");
    resolve::resolve(&idl, &pp.defines, &Overrides::default(), &topic_map, &qos_map)
}

#[test]
fn scan_qos_map_lifts_known_topic_profiles() {
    let root = repo_root();
    let idl_path = root.join("docs/compliance/tms/mil-std-3071_data_model.idl");
    let raw = std::fs::read_to_string(&idl_path).expect("read IDL");
    let pp = parser::preprocess(&raw);
    let map = resolve::scan_qos_map(&pp.source);

    assert_eq!(map.get("Heartbeat").map(String::as_str), Some("Medium"));
    assert_eq!(
        map.get("DeviceInfo").map(String::as_str),
        Some("PublishLast")
    );
    assert_eq!(map.get("Reply").map(String::as_str), Some("Reply"));
    assert_eq!(
        map.get("PowerSwitchRequest").map(String::as_str),
        Some("Command")
    );
    assert_eq!(
        map.get("AcSummaryMeasurementUpdate").map(String::as_str),
        Some("Continuous")
    );
    assert_eq!(map.get("StorageUpdate").map(String::as_str), Some("Slow"));
    assert_eq!(
        map.get("AuthorizationToEnergizeReply").map(String::as_str),
        Some("Response")
    );
}

#[test]
fn full_idl_topic_qos_covers_every_topic_with_a_valid_d5_profile() {
    let root = repo_root();
    let gen = tms_idl_codegen::generate(&root, &overrides_path()).expect("full IDL parses clean");

    let distinct_topics: HashSet<String> = gen
        .model
        .types
        .iter()
        .flat_map(|t| t.topics.iter().cloned())
        .collect();
    assert_eq!(
        gen.model.topic_qos.len(),
        distinct_topics.len(),
        "every distinct topic must have exactly one QoS entry"
    );
    for q in &gen.model.topic_qos {
        assert!(
            distinct_topics.contains(&q.topic),
            "topic_qos entry `{}` is not a real topic",
            q.topic
        );
    }
}

#[test]
fn non_appendable_extensibility_is_a_hard_error() {
    let src = r#"
        module tms {
            @extensibility(MUTABLE)
            struct Bad { uint32 x; };
        };
    "#;
    let err = resolve_snippet_result(src).expect_err("MUTABLE must be a hard error");
    assert!(err.contains("Bad"), "error should name the type: {err}");
    assert!(err.contains("MUTABLE"), "{err}");
}

#[test]
fn final_extensibility_is_a_hard_error() {
    let src = r#"
        module tms {
            @extensibility(FINAL)
            enum Bad { A, B };
        };
    "#;
    let err = resolve_snippet_result(src).expect_err("FINAL must be a hard error");
    assert!(err.contains("Bad"), "{err}");
    assert!(err.contains("FINAL"), "{err}");
}

#[test]
fn missing_extensibility_is_a_hard_error() {
    let src = r#"
        module tms {
            struct Bad { uint32 x; };
        };
    "#;
    let err = resolve_snippet_result(src).expect_err("missing @extensibility must be a hard error");
    assert!(err.contains("Bad"), "{err}");
}

#[test]
fn unknown_qos_profile_name_is_a_hard_error() {
    let src = r#"
        module tms {
            module topic {
                /**
                 * QoS Profile: Fast
                 * Data Type: tms::Widget
                 */
                const string TOPIC_WIDGET = "Widget";
            };
            @extensibility(APPENDABLE)
            struct Widget { uint32 x; };
        };
    "#;
    let err =
        resolve_snippet_result(src).expect_err("unknown QoS profile name must be a hard error");
    assert!(err.contains("Widget"), "{err}");
    assert!(err.contains("Fast"), "{err}");
}

#[test]
fn missing_qos_profile_is_a_hard_error() {
    let src = r#"
        module tms {
            module topic {
                /**
                 * Data Type: tms::Widget
                 */
                const string TOPIC_WIDGET = "Widget";
            };
            @extensibility(APPENDABLE)
            struct Widget { uint32 x; };
        };
    "#;
    let err =
        resolve_snippet_result(src).expect_err("missing QoS Profile line must be a hard error");
    assert!(err.contains("Widget"), "{err}");
}

#[test]
fn emitted_generated_qos_has_expected_surface() {
    let root = repo_root();
    let gen = tms_idl_codegen::generate(&root, &overrides_path()).expect("full IDL parses clean");
    let out = &gen.generated_qos;

    assert!(out.starts_with("// @generated by tms-idl-codegen"));
    assert!(out.contains("pub enum QosProfile"));
    assert!(out.contains("PublishLast"));
    assert!(out.contains("Rare"));
    assert!(out.contains("pub struct QosPolicies"));
    assert!(out.contains("pub const fn policies"));
    assert!(out.contains("pub const fn qos_profile"));
    assert!(out.contains("pub const fn key_fields"));
    assert!(out.contains("use crate::TmsTopicId"));
    // A known topic -> profile binding, spot-checked in the emitted match arm text.
    assert!(out.contains("TmsTopicId::Heartbeat => QosProfile::Medium"));
    assert!(out.contains("TmsTopicId::Reply => QosProfile::Reply"));

    // Must parse as valid Rust (the emitter already proved this via syn::parse2, but a second,
    // independent parse here catches any post-formatting corruption).
    syn::parse_file(out).expect("generated_qos.rs must be valid Rust");

    // Keep a copy for inspection (NOT under crates/ — the main loop wires the real file).
    let tmp = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("generated_qos.rs");
    std::fs::write(&tmp, out).unwrap();
}

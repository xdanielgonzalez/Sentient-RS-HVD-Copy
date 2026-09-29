//! TMS IDL code generator — library surface (parser, resolver, emitters) so the binary AND the
//! integration tests share one implementation.
//!
//! The generator emits several `@generated` files (DUAL-TARGET, the split that dissolves the
//! layering cycle): `crates/lib-tms/src/generated.rs` (the pure MIL-STD-3071 model — types + IDL
//! typedefs + `TmsTopic`/`TmsTopicId` + the `TmsMessage` factory), `generated_fields.rs` (the
//! per-topic field-accessor chains), `generated_qos.rs` (the 8 Annex D.5 `QosProfile`s + per-topic
//! profile/key-field maps) and `crates/lib-mapping/src/wire/generated_encoders.rs` (the bridge — the
//! flat `WireFlat` canonical->wire encoders + `FAULT_TABLE` + the `TopicRings` render-on-output
//! store). It also emits the traceability report. (It REPLACES the earlier `const` descriptor
//! registry + reflective `DynamicValue` codec.)

pub mod ast;
pub mod emit_cdr;
pub mod emit_encoders;
pub mod emit_fields;
pub mod emit_qos;
pub mod emit_types;
pub mod model;
pub mod overrides;
pub mod parser;
pub mod report;
pub mod resolve;

use std::path::Path;

use model::Model;
use overrides::Overrides;

/// The generated artifacts + a parse summary. DUAL-TARGET: the pure TMS model goes to `lib-tms`, the
/// canonical->wire encoders + `TopicRings` go to `lib-mapping` (the bridge) — this split is what
/// dissolves the layering cycle (see the crate-level docs).
pub struct Generated {
    /// The `@generated` TMS type catalogue (`crates/lib-tms/src/generated.rs`): types + typedefs +
    /// `TmsTopicId` + `TmsMessage` factory. No encoders, no canonical/JsonSink deps.
    pub generated_tms: String,
    /// The `@generated` bridge file (`crates/lib-mapping/src/wire/generated_encoders.rs`): the flat
    /// `WireFlat` encoders + `FAULT_TABLE` + the `TopicRings` render-on-output store.
    pub generated_encoders: String,
    /// The `@generated` field-chain file (`crates/lib-tms/src/generated_fields.rs`): per-topic
    /// `<T>Src` accessor traits + `Field` ZSTs + `Chain` node lists + `HasChain` impls for the
    /// `[subset] chain_topics` transitive closure.
    pub generated_fields: String,
    /// The `@generated` QoS catalogue (`crates/lib-tms/src/generated_qos.rs`): the 8 MIL-STD-3071
    /// Annex D.5 `QosProfile`s + their `QosPolicies` (plain data), the per-topic profile map, and
    /// per-topic DDS instance-key field metadata.
    pub generated_qos: String,
    /// The `@generated` XCDR2 CDR chains (`crates/lib-tms/src/generated_cdr.rs`): per-topic
    /// `<T>CdrSrc` accessor traits + `CdrField` ZSTs + `CdrChain`/`KeyChain` node lists +
    /// `HasCdrChain` impls for the `[subset] cdr_topics` transitive closure.
    pub generated_cdr: String,
    /// The `@generated` battery canonical->CDR view bindings
    /// (`crates/lib-mapping/src/wire/generated_cdr_encoders.rs`): the CDR twin of
    /// `generated_encoders.rs` with UNSCALED values.
    pub generated_cdr_encoders: String,
    pub report_json: String,
    pub report_md: String,
    pub model: Model,
    pub struct_count: usize,
    pub enum_count: usize,
    pub topic_count: usize,
}

/// Parse the IDL at `root/docs/compliance/tms/…`, resolve it, and produce every generated artifact.
pub fn generate(root: &Path, overrides_path: &Path) -> Result<Generated, String> {
    use model::RKind;

    let idl_path = root.join("docs/compliance/tms/mil-std-3071_data_model.idl");
    let raw = std::fs::read_to_string(&idl_path)
        .map_err(|e| format!("cannot read IDL {}: {e}", idl_path.display()))?;
    let overrides = Overrides::load(&overrides_path.to_string_lossy())?;

    let pp = parser::preprocess(&raw);
    let topic_map = resolve::scan_topic_map(&pp.source);
    let qos_map = resolve::scan_qos_map(&pp.source);
    let idl = parser::parse_idl(&pp.source)?;
    let model = resolve::resolve(&idl, &pp.defines, &overrides, &topic_map, &qos_map)?;

    let struct_count = model
        .types
        .iter()
        .filter(|t| t.kind == RKind::Struct)
        .count();
    let enum_count = model.types.iter().filter(|t| t.kind == RKind::Enum).count();
    let topic_count: usize = topic_map.values().map(|v| v.len()).sum();

    let generated_tms = emit_types::emit_tms(&model, &overrides)?;
    let generated_encoders = emit_encoders::emit_mapping(&model, &overrides)?;
    let generated_fields = emit_fields::emit_fields(&model, &overrides)?;
    let generated_qos = emit_qos::emit_qos(&model)?;
    let generated_cdr = emit_cdr::emit_cdr(&model, &overrides)?;
    let generated_cdr_encoders = emit_cdr::emit_cdr_encoders(&model, &overrides)?;
    let (report_json, report_md) = report::emit_report(&model);

    Ok(Generated {
        generated_tms,
        generated_encoders,
        generated_fields,
        generated_qos,
        generated_cdr,
        generated_cdr_encoders,
        report_json,
        report_md,
        model,
        struct_count,
        enum_count,
        topic_count,
    })
}

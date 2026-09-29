//! Two-pass resolution: AST -> [`Model`]. Pass 1 collects every symbol (consts, typedefs, struct /
//! enum names + their modules) so refs resolve regardless of declaration order; pass 2 builds the
//! resolved [`RType`]s (typedefs chased, bounds evaluated + override-narrowed, names snake_cased,
//! ac/dc name collisions disambiguated).

use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::model::*;
use crate::overrides::Overrides;

struct Ctx<'a> {
    consts: HashMap<String, Expr>,
    typedefs: HashMap<String, TypeRef>,
    struct_names: HashSet<String>,
    enum_names: HashSet<String>,
    /// simple name -> the module leaves (`ac`/`dc`/`tms`) it is declared in (to find collisions).
    name_modules: HashMap<String, Vec<String>>,
    defines: &'a HashMap<String, f64>,
    overrides: &'a Overrides,
    topic_map: &'a HashMap<String, Vec<String>>,
}

enum Ordered<'a> {
    Struct(String, &'a StructDef),
    Enum(String, &'a EnumDef),
}

pub fn resolve(
    idl: &Idl,
    defines: &HashMap<String, f64>,
    overrides: &Overrides,
    topic_map: &HashMap<String, Vec<String>>,
    qos_map: &HashMap<String, String>,
) -> Result<Model, String> {
    let mut ctx = Ctx {
        consts: HashMap::new(),
        typedefs: HashMap::new(),
        struct_names: HashSet::new(),
        enum_names: HashSet::new(),
        name_modules: HashMap::new(),
        defines,
        overrides,
        topic_map,
    };
    let mut ordered = Vec::new();
    collect(&idl.items, "tms", &mut ctx, &mut ordered, true);

    let mut types = Vec::new();
    for item in &ordered {
        match item {
            Ordered::Struct(module, s) => types.push(resolve_struct(module, s, &ctx)?),
            Ordered::Enum(module, e) => types.push(resolve_enum(module, e, overrides)?),
        }
    }

    // The KEPT primitive typedefs (allow-listed in the overrides) emitted as named `pub type`
    // aliases into lib-tms. RHS resolved from the IDL (typedefs chased, bounds evaluated), so a
    // rename/re-bound in the standard flows through automatically. Emitted in allow-list order.
    let mut typedefs = Vec::new();
    for name in &overrides.typedefs.emit {
        let td = ctx
            .typedefs
            .get(name.as_str())
            .ok_or_else(|| format!("[typedefs] emit lists `{name}`, not an IDL typedef"))?;
        let ty = resolve_type(td, "tms", &ctx, 0)
            .map_err(|e| format!("typedef {name}: {e}"))?;
        typedefs.push(RTypedef {
            name: name.clone(),
            ty,
        });
    }

    let topic_qos = build_topic_qos(&types, qos_map)?;

    Ok(Model {
        types,
        typedefs,
        topic_qos,
    })
}

/// Every distinct topic — walked in the same declaration order `emit_types::ordered_topics` uses
/// (over `types`, then each type's `topics`), so [`Model::topic_qos`] lines up 1:1 with the emitted
/// `TmsTopicId` discriminant order — paired with its validated D.5 QoS profile. Hard-errors, naming
/// the offending topic, if a topic has no `QoS Profile:` doc-comment line or the line names anything
/// but one of the 8 MIL-STD-3071 Annex D.5 profiles.
fn build_topic_qos(
    types: &[RType],
    qos_map: &HashMap<String, String>,
) -> Result<Vec<RTopicQos>, String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for t in types {
        for topic in &t.topics {
            if !seen.insert(topic.clone()) {
                continue;
            }
            let raw = qos_map
                .get(topic)
                .ok_or_else(|| format!("topic `{topic}`: no `QoS Profile:` doc-comment line in the IDL"))?;
            let profile = RQosProfile::parse(raw).ok_or_else(|| {
                let names: Vec<&str> = RQosProfile::ALL.iter().map(|p| p.ident()).collect();
                format!(
                    "topic `{topic}`: unknown QoS profile `{raw}` — expected one of {}",
                    names.join(", ")
                )
            })?;
            out.push(RTopicQos {
                topic: topic.clone(),
                profile,
            });
        }
    }
    Ok(out)
}

/// Parse `@extensibility(...)` off a type's annotations, hard-erroring on anything but APPENDABLE —
/// the uniform-DHEADER assumption the owned CDR chain (WP2/WP3) pins on. Every one of the 136
/// MIL-STD-3071 IDL types is `@extensibility(APPENDABLE)` today; a future `FINAL`/`MUTABLE` type (or
/// one missing the annotation) fails codegen loudly instead of silently mis-encoding.
fn resolve_extensibility(anns: &[Ann], type_name: &str, idl_line: u32) -> Result<RExtensibility, String> {
    let raw = anns.iter().find_map(|a| match a {
        Ann::Extensibility(s) => Some(s.as_str()),
        _ => None,
    });
    match raw {
        Some("APPENDABLE") => Ok(RExtensibility::Appendable),
        Some("FINAL") => Err(format!(
            "{type_name} (idl:{idl_line}): @extensibility(FINAL) is not supported — the owned CDR \
             chain assumes uniform APPENDABLE (DHEADER-delimited) encoding; only APPENDABLE is \
             implemented"
        )),
        Some("MUTABLE") => Err(format!(
            "{type_name} (idl:{idl_line}): @extensibility(MUTABLE) is not supported — the owned CDR \
             chain assumes uniform APPENDABLE (DHEADER-delimited) encoding; PL_CDR/member-ID handling \
             is not implemented"
        )),
        Some(other) => Err(format!(
            "{type_name} (idl:{idl_line}): unknown @extensibility(`{other}`) — expected APPENDABLE"
        )),
        None => Err(format!(
            "{type_name} (idl:{idl_line}): missing @extensibility annotation — every struct/enum \
             must be explicitly @extensibility(APPENDABLE)"
        )),
    }
}

/// Pass 1 — walk the module tree collecting symbols + an ordered type list.
fn collect<'a>(
    items: &'a [Item],
    module: &str,
    ctx: &mut Ctx,
    ordered: &mut Vec<Ordered<'a>>,
    top: bool,
) {
    for item in items {
        match item {
            Item::Module(m) => {
                // The top-level file item IS `module tms`; its children are the real `tms` scope.
                let child = if top && m.name == "tms" {
                    "tms".to_string()
                } else {
                    format!("{module}::{}", m.name)
                };
                collect(&m.items, &child, ctx, ordered, false);
            }
            Item::Const(c) => {
                ctx.consts.insert(c.name.clone(), c.value.clone());
            }
            Item::Typedef(t) => {
                ctx.typedefs.insert(t.name.clone(), t.ty.clone());
            }
            Item::Struct(s) => {
                ctx.struct_names.insert(s.name.clone());
                note_module(&mut ctx.name_modules, &s.name, module);
                ordered.push(Ordered::Struct(module.to_string(), s));
            }
            Item::Enum(e) => {
                ctx.enum_names.insert(e.name.clone());
                note_module(&mut ctx.name_modules, &e.name, module);
                ordered.push(Ordered::Enum(module.to_string(), e));
            }
        }
    }
}

fn note_module(map: &mut HashMap<String, Vec<String>>, name: &str, module: &str) {
    let leaf = module_leaf(module).to_string();
    let e = map.entry(name.to_string()).or_default();
    if !e.contains(&leaf) {
        e.push(leaf);
    }
}

fn module_leaf(module: &str) -> &str {
    module.rsplit("::").next().unwrap_or(module)
}

fn resolve_struct(module: &str, s: &StructDef, ctx: &Ctx) -> Result<RType, String> {
    let extensibility = resolve_extensibility(&s.anns, &s.name, s.line)?;
    let mut fields = Vec::new();
    for m in &s.members {
        let ty = resolve_type(&m.ty, module, ctx, 0)
            .map_err(|e| format!("{}.{}: {e}", s.name, m.name))?;
        let bound = leaf_bound(&ty);
        let (optional, is_key, unit, range) = extract_member_anns(&m.anns, ctx.defines);
        fields.push(RField {
            name: snake_case(&m.name),
            idl_name: m.name.clone(),
            ty,
            optional,
            is_key,
            unit,
            range,
            bound,
        });
    }
    let qualified = format!("{module}::{}", s.name);
    let topics = ctx.topic_map.get(&qualified).cloned().unwrap_or_default();
    Ok(RType {
        name: s.name.clone(),
        module: module.to_string(),
        idl_line: s.line,
        kind: RKind::Struct,
        topics,
        fields,
        variants: Vec::new(),
        extensibility,
    })
}

fn resolve_enum(module: &str, e: &EnumDef, overrides: &Overrides) -> Result<RType, String> {
    let extensibility = resolve_extensibility(&e.anns, &e.name, e.line)?;
    let rust = enum_rust_names(&e.variants);
    let variants = e
        .variants
        .iter()
        .enumerate()
        .map(|(i, wire)| RVariant {
            // A hand-curated override (golden semantic name) wins over the mechanical de-prefix; the
            // wire spelling (`wire_name`) is untouched, so bytes are identical.
            rust_name: overrides
                .variant_idents
                .get(wire)
                .cloned()
                .unwrap_or_else(|| rust[i].clone()),
            wire_name: wire.clone(),
            ordinal: i as u8,
        })
        .collect();
    Ok(RType {
        name: e.name.clone(),
        module: module.to_string(),
        idl_line: e.line,
        kind: RKind::Enum,
        topics: Vec::new(),
        fields: Vec::new(),
        variants,
        extensibility,
    })
}

fn resolve_type(ty: &TypeRef, referrer: &str, ctx: &Ctx, depth: u32) -> Result<RFieldType, String> {
    if depth > 32 {
        return Err("type resolution too deep (cycle?)".into());
    }
    match ty {
        TypeRef::Sequence(elem, b) => Ok(RFieldType::Seq(
            Box::new(resolve_type(elem, referrer, ctx, depth + 1)?),
            resolve_bound(b, ctx),
        )),
        TypeRef::BoundedString(b) => Ok(RFieldType::Str(resolve_bound(b, ctx))),
        TypeRef::CharArray(b) => Ok(RFieldType::Str(resolve_bound(b, ctx))),
        TypeRef::Named(s) => {
            let base = simple(s);
            if let Some(p) = primitive(base) {
                return Ok(p);
            }
            if let Some(td) = ctx.typedefs.get(base) {
                return resolve_type(td, referrer, ctx, depth + 1);
            }
            if ctx.struct_names.contains(base) || ctx.enum_names.contains(base) {
                return Ok(RFieldType::Ref(qualify_ref(base, referrer, ctx)));
            }
            Err(format!("unknown type `{s}`"))
        }
    }
}

/// Disambiguate a struct/enum reference when the simple name is reused across `ac`/`dc`.
fn qualify_ref(base: &str, referrer: &str, ctx: &Ctx) -> String {
    let modules = ctx.name_modules.get(base);
    let ambiguous = modules.map(|m| m.len() > 1).unwrap_or(false);
    if !ambiguous {
        return base.to_string();
    }
    let referrer_leaf = module_leaf(referrer);
    let target_leaf = modules
        .and_then(|m| m.iter().find(|l| *l == referrer_leaf))
        .or_else(|| modules.and_then(|m| m.first()))
        .cloned()
        .unwrap_or_else(|| referrer_leaf.to_string());
    format!("{target_leaf}::{base}")
}

fn leaf_bound(ty: &RFieldType) -> Option<u32> {
    match ty {
        RFieldType::Str(n) | RFieldType::Seq(_, n) => Some(*n),
        _ => None,
    }
}

fn resolve_bound(b: &Bound, ctx: &Ctx) -> u32 {
    match b {
        Bound::Lit(n) => *n as u32,
        Bound::Ident(name) => const_value(simple(name), ctx, 0) as u32,
    }
}

fn const_value(name: &str, ctx: &Ctx, depth: u32) -> u64 {
    if depth > 32 {
        return 0;
    }
    if let Some(v) = ctx.overrides.bounds.get(name) {
        return *v;
    }
    match ctx.consts.get(name) {
        Some(Expr::Int(n)) => (*n).max(0) as u64,
        Some(Expr::Ident(x)) => const_value(simple(x), ctx, depth + 1),
        _ => 0,
    }
}

fn extract_member_anns(
    anns: &[Ann],
    defines: &HashMap<String, f64>,
) -> (bool, bool, Option<String>, Option<(f32, f32)>) {
    let mut optional = false;
    let mut is_key = false;
    let mut unit = None;
    let mut range = None;
    for a in anns {
        match a {
            Ann::Optional => optional = true,
            Ann::Key => is_key = true,
            Ann::Unit(u) => unit = Some(u.clone()),
            Ann::Range(lo, hi) => {
                range = Some((rangeval_f32(lo, defines), rangeval_f32(hi, defines)))
            }
            Ann::Min(v) => range = Some((rangeval_f32(v, defines), f32::INFINITY)),
            Ann::Max(v) => range = Some((f32::NEG_INFINITY, rangeval_f32(v, defines))),
            _ => {}
        }
    }
    (optional, is_key, unit, range)
}

fn rangeval_f32(rv: &RangeVal, defines: &HashMap<String, f64>) -> f32 {
    match rv {
        RangeVal::Num(n) => *n as f32,
        RangeVal::Ident { name, negated } => {
            let v = defines.get(name).copied().unwrap_or(0.0);
            (if *negated { -v } else { v }) as f32
        }
    }
}

/// Last `::`-segment of a possibly-qualified name.
fn simple(name: &str) -> &str {
    name.rsplit("::").next().unwrap_or(name)
}

fn primitive(base: &str) -> Option<RFieldType> {
    Some(match base {
        "float" => RFieldType::F32,
        "double" => RFieldType::F64,
        "boolean" => RFieldType::Bool,
        "octet" | "uint8" => RFieldType::U8,
        "uint16" => RFieldType::U16,
        "uint32" => RFieldType::U32,
        "uint64" => RFieldType::U64,
        "int16" | "short" | "int8" => RFieldType::I16,
        "int32" | "long" => RFieldType::I32,
        "char" => RFieldType::U8,
        _ => return None,
    })
}

/// camelCase (IDL) -> snake_case (golden Rust field name; `serde` emits it verbatim).
pub fn snake_case(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len() + 4);
    for (i, &c) in chars.iter().enumerate() {
        if c.is_ascii_uppercase() {
            let prev_low_digit =
                i > 0 && (chars[i - 1].is_ascii_lowercase() || chars[i - 1].is_ascii_digit());
            let prev_up = i > 0 && chars[i - 1].is_ascii_uppercase();
            let next_low = i + 1 < chars.len() && chars[i + 1].is_ascii_lowercase();
            if i > 0 && (prev_low_digit || (prev_up && next_low)) {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// Rust variant names: strip the shared `XX_` type-prefix AND any leading `<digits>_` ordinal, then
/// PascalCase the rest — reproducing the golden's de-ordinalized spellings exactly (`SEV_7_MAJOR` ->
/// `Major`, `SEV_1_CLEAR` -> `Clear`, `ROLE_STORAGE` -> `Storage`, `FMI_CONDITION_EXISTS` ->
/// `ConditionExists`). The JSON wire spelling is the `#[serde(rename)]` (the full IDL enumerator,
/// UNCHANGED), so the Rust identifier is free to be cleaned without touching the bytes. NOTE: a few
/// golden `FailureModeIndicator` idents are hand-curated semantic names (`FMI_HIGH_MOST_SEVERE_HIGH`
/// -> `AboveNormalMostSevere`) not mechanically recoverable; those variants are not referenced by any
/// consumer, so the mechanical spelling is used for them (proven acceptable by the workspace compile).
fn enum_rust_names(variants: &[String]) -> Vec<String> {
    let prefix = common_prefix_to_underscore(variants);
    variants
        .iter()
        .map(|v| {
            let stripped = v.strip_prefix(&prefix).unwrap_or(v);
            pascal_case(strip_leading_ordinal(stripped))
        })
        .collect()
}

/// Strip a leading `<digits>_` ordinal segment (`7_MAJOR` -> `MAJOR`, `10_HUMAN_SAFETY` ->
/// `HUMAN_SAFETY`) so the golden de-ordinalized spelling is reproduced (not the `V7Major` a bare
/// PascalCase of `7_MAJOR` would yield). Only strips a pure-digit run immediately followed by `_`
/// and more text — a variant like `2D` (no trailing `_`) is left for `pascal_case` to `V`-prefix.
fn strip_leading_ordinal(s: &str) -> &str {
    if let Some(us) = s.find('_') {
        if us > 0 && s[..us].bytes().all(|b| b.is_ascii_digit()) && us + 1 < s.len() {
            return &s[us + 1..];
        }
    }
    s
}

fn common_prefix_to_underscore(variants: &[String]) -> String {
    let Some(first) = variants.first() else {
        return String::new();
    };
    let mut prefix = first.clone();
    for v in variants {
        while !v.starts_with(&prefix) && !prefix.is_empty() {
            prefix.pop();
        }
    }
    match prefix.rfind('_') {
        Some(i) => prefix[..=i].to_string(),
        None => String::new(),
    }
}

fn pascal_case(s: &str) -> String {
    let mut out = String::new();
    for seg in s.split('_') {
        let mut ch = seg.chars();
        if let Some(f) = ch.next() {
            out.push(f.to_ascii_uppercase());
            out.extend(ch.map(|c| c.to_ascii_lowercase()));
        }
    }
    if out
        .chars()
        .next()
        .map(|c| c.is_ascii_digit())
        .unwrap_or(true)
    {
        out.insert(0, 'V');
    }
    out
}

/// Scan the raw IDL `topic` module for `Data Type: tms::X` -> `const string TOPIC_* = "Name";`
/// associations (the doc-comment-carried topic<->type binding my grammar skips as a comment).
pub fn scan_topic_map(source: &str) -> HashMap<String, Vec<String>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    let mut pending: Option<String> = None;
    for line in source.lines() {
        if let Some(idx) = line.find("Data Type:") {
            let rest = &line[idx + "Data Type:".len()..];
            if let Some(tok) = rest.split_whitespace().next() {
                pending = Some(tok.to_string());
            }
        } else if line.trim_start().starts_with("const string TOPIC") {
            if let (Some(q1), Some(t)) = (line.find('"'), pending.take()) {
                if let Some(q2) = line[q1 + 1..].find('"') {
                    let name = &line[q1 + 1..q1 + 1 + q2];
                    map.entry(t).or_default().push(name.to_string());
                }
            }
        }
    }
    map
}

/// Scan the raw IDL `topic` module for `QoS Profile: X` -> `const string TOPIC_* = "Name";`
/// associations — the same doc-comment block [`scan_topic_map`] lifts `Data Type:` from, a different
/// line. The raw profile token is validated (against the 8 D.5 names) by [`resolve`]'s
/// `build_topic_qos`, not here — this is a pure lift, mirroring `scan_topic_map` exactly.
pub fn scan_qos_map(source: &str) -> HashMap<String, String> {
    let mut map: HashMap<String, String> = HashMap::new();
    let mut pending: Option<String> = None;
    for line in source.lines() {
        if let Some(idx) = line.find("QoS Profile:") {
            let rest = &line[idx + "QoS Profile:".len()..];
            if let Some(tok) = rest.split_whitespace().next() {
                pending = Some(tok.to_string());
            }
        } else if line.trim_start().starts_with("const string TOPIC") {
            if let (Some(q1), Some(profile)) = (line.find('"'), pending.take()) {
                if let Some(q2) = line[q1 + 1..].find('"') {
                    let name = &line[q1 + 1..q1 + 1 + q2];
                    map.insert(name.to_string(), profile);
                }
            }
        }
    }
    map
}

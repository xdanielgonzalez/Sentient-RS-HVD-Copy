//! The RESOLVED descriptor model (owned mirror of `lib_tms_model`'s `const` types). Typedefs are
//! chased, bounds evaluated (+ override-narrowed), forward refs bound, field names snake_cased. The
//! emitter turns this into the `const TMS_REGISTRY` and the flat encoders.

#[derive(Debug, Clone)]
pub struct Model {
    /// Structs + enums, in IDL declaration order.
    pub types: Vec<RType>,
    /// The KEPT primitive typedefs to emit as named `pub type` aliases into `lib-tms` (selected by
    /// the `[typedefs] emit` override allow-list; RHS resolved from the IDL so it cannot drift).
    pub typedefs: Vec<RTypedef>,
    /// Every distinct topic (in `TmsTopicId` discriminant order — the same declaration-order walk
    /// `emit_types::ordered_topics` performs) paired with its D.5 QoS profile, lifted from the IDL
    /// topic-module `QoS Profile:` doc-comment line and validated by [`crate::resolve::resolve`].
    pub topic_qos: Vec<RTopicQos>,
}

/// A resolved IDL typedef emitted as a named `pub type` alias (e.g. `Identity = heapless::String<32>`).
#[derive(Debug, Clone)]
pub struct RTypedef {
    pub name: String,
    pub ty: RFieldType,
}

#[derive(Debug, Clone)]
pub struct RType {
    pub name: String,
    pub module: String,
    pub idl_line: u32,
    pub kind: RKind,
    pub topics: Vec<String>,
    pub fields: Vec<RField>,
    pub variants: Vec<RVariant>,
    /// This type's `@extensibility(...)`, validated by [`crate::resolve::resolve`] — construction
    /// hard-errors before an `RType` with anything but [`RExtensibility::Appendable`] exists (the
    /// executable pin; see the enum doc).
    pub extensibility: RExtensibility,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RKind {
    Struct,
    Enum,
}

/// DDS-XTypes `@extensibility` (DDSI-RTPS/XCDR2 wire-shape family: FINAL = plain CDR, no DHEADER;
/// APPENDABLE = CDR + a length-prefixed DHEADER, member order fixed; MUTABLE = PL_CDR, per-member
/// IDs). ALL 136 MIL-STD-3071 IDL types are `@extensibility(APPENDABLE)` today — the owned CDR chain
/// (WP2/WP3) assumes this uniformly (one DHEADER shape, no member-ID/PL_CDR handling). `resolve.rs`
/// therefore hard-errors on anything but `Appendable`; the other two variants exist so that error can
/// name what it actually saw, not because this codegen implements them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RExtensibility {
    Final,
    Appendable,
    Mutable,
}

/// A topic's MIL-STD-3071 Annex D.5 QoS profile, lifted from the IDL topic-module doc-comment
/// `QoS Profile:` line. `profile` is validated (by [`crate::resolve::resolve`]) to be one of the 8
/// named D.5 profiles — never a raw/unchecked string.
#[derive(Debug, Clone)]
pub struct RTopicQos {
    pub topic: String,
    pub profile: RQosProfile,
}

/// The 8 named DDS QoS profiles MIL-STD-3071 Annex D.5 defines. Mirrors
/// `lib_tms::generated_qos::QosProfile`'s variant set + spelling exactly — `emit_qos.rs` mints that
/// enum's identifiers straight from [`RQosProfile::ident`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RQosProfile {
    PublishLast,
    Rare,
    Slow,
    Medium,
    Continuous,
    Command,
    Response,
    Reply,
}

impl RQosProfile {
    /// Every profile, in the fixed D.5 enumeration order (also the emitted enum's declaration/
    /// discriminant order).
    pub const ALL: [RQosProfile; 8] = [
        RQosProfile::PublishLast,
        RQosProfile::Rare,
        RQosProfile::Slow,
        RQosProfile::Medium,
        RQosProfile::Continuous,
        RQosProfile::Command,
        RQosProfile::Response,
        RQosProfile::Reply,
    ];

    /// Parse the exact D.5 profile name as it appears in the IDL `QoS Profile:` doc-comment line.
    /// `None` for anything else — the caller turns that into a hard codegen error naming the
    /// offending topic (per the plan: "unknown name ... = codegen error with the topic named").
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.ident() == name)
    }

    /// The exact spelling — the doc-comment token, this model's `Debug` name, and the emitted
    /// `QosProfile` Rust variant identifier are all the same string by construction.
    pub const fn ident(self) -> &'static str {
        match self {
            RQosProfile::PublishLast => "PublishLast",
            RQosProfile::Rare => "Rare",
            RQosProfile::Slow => "Slow",
            RQosProfile::Medium => "Medium",
            RQosProfile::Continuous => "Continuous",
            RQosProfile::Command => "Command",
            RQosProfile::Response => "Response",
            RQosProfile::Reply => "Reply",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RField {
    pub name: String,
    pub idl_name: String,
    pub ty: RFieldType,
    pub optional: bool,
    pub is_key: bool,
    pub unit: Option<String>,
    pub range: Option<(f32, f32)>,
    pub bound: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RFieldType {
    F32,
    F64,
    U8,
    U16,
    U32,
    I16,
    I32,
    U64,
    Bool,
    Str(u32),
    Seq(Box<RFieldType>, u32),
    /// Reference to a struct/enum, by (possibly `sub::`-qualified) registry name.
    Ref(String),
}

#[derive(Debug, Clone)]
pub struct RVariant {
    pub rust_name: String,
    pub wire_name: String,
    pub ordinal: u8,
}

impl Model {
    pub fn find(&self, name: &str) -> Option<&RType> {
        self.types.iter().find(|t| t.name == name)
    }
}

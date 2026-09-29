use dust_dds::infrastructure::type_support::TypeSupport;

/// dust_dds patch of `tms::TopicInfo` — `@nested @extensibility(APPENDABLE)` (idl:4116), member-only
/// (`DeviceInfo.topics`). The three required `published_*`/`supported_*` members plus the `@optional
/// extension_topics` are all `sequence<string>` — non-primitive-element sequences carrying the
/// rule-(12) NEXTINT delimiter (vendored dust_dds patch; `vendor/dust_dds/PATCHES.md`).
/// `extension_topics` is `@optional sequence<string>` → `Option<Vec<String>>` +
/// `#[dust_dds(optional)]` (wp-derive; the chain uses `emit_optional_seq` → boolean-then-sequence).
/// Field order = the IDL member order, matching `lib_tms::generated::TopicInfo`. Derive-only: no hand
/// impl, so this is a single-type file by design.
#[derive(TypeSupport, Clone, Debug, PartialEq)]
#[dust_dds(nested, extensibility = "appendable")]
pub struct TopicInfo {
    pub data_model_version: String,
    pub published_conditional_topics: Vec<String>,
    pub published_optional_topics: Vec<String>,
    pub supported_request_topics: Vec<String>,
    #[dust_dds(optional)]
    pub extension_topics: Option<Vec<String>>,
}

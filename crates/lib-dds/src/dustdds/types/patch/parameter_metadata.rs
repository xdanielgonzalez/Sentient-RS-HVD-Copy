use dust_dds::infrastructure::type_support::TypeSupport;

/// dust_dds patch of `tms::ParameterMetadata` — `@nested @extensibility(APPENDABLE)` (idl:2983),
/// member-only (`DeviceInfo.control_parameters`/`metric_parameters`, each an `@optional
/// sequence<ParameterMetadata>`). All members required. `enum_labels` is a `sequence<string>` — a
/// non-primitive-element sequence carrying the rule-(12) NEXTINT delimiter (vendored dust_dds patch;
/// `vendor/dust_dds/PATCHES.md`). `String`/`Vec` (not `heapless`) per the patch std-only rule. Field
/// order = the IDL member order, matching `lib_tms::generated::ParameterMetadata`. Derive-only: no
/// hand impl, so this is a single-type file by design.
#[derive(TypeSupport, Clone, Debug, PartialEq)]
#[dust_dds(nested, extensibility = "appendable")]
pub struct ParameterMetadata {
    pub name: String,
    pub units: String,
    pub nominal_min_value: f32,
    pub nominal_max_value: f32,
    pub hard_min_value: f32,
    pub hard_max_value: f32,
    pub resolution: f32,
    pub enum_labels: Vec<String>,
}

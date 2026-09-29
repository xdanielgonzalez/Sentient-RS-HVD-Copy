use dust_dds::infrastructure::type_support::TypeSupport;

/// dust_dds patch of `tms::Heartbeat` — `{ string device_id @key; uint32 sequence_number; }`,
/// APPENDABLE (uniform delimited XCDR2, matching every TMS type's `@extensibility(APPENDABLE)`). The
/// struct name is the DDS type name (`get_type_name()` = "Heartbeat"), matching the topic + the
/// CycloneDDS interop type. Field order = the IDL member order (the chains' order). Derive-only: no
/// hand impl, so this is a single-type file by design.
#[derive(TypeSupport, Clone, Debug, PartialEq)]
#[dust_dds(extensibility = "appendable")]
pub struct Heartbeat {
    #[dust_dds(key)]
    pub device_id: String,
    pub sequence_number: u32,
}

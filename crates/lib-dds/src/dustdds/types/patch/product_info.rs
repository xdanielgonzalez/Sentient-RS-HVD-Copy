use dust_dds::infrastructure::type_support::TypeSupport;

/// dust_dds patch of `tms::ProductInfo` — `@nested @extensibility(APPENDABLE)` (idl:4279), member-only
/// (`DeviceInfo.product`). `platform_id` is `@optional string` → `Option<String>` +
/// `#[dust_dds(optional)]` (wp-derive; XCDR2 presence flag from wp-serializer). All other members are
/// required strings (`String`, not `heapless`, per the patch std-only rule). Field order = the IDL
/// member order, matching `lib_tms::generated::ProductInfo`. Derive-only: no hand impl, so this is a
/// single-type file by design.
#[derive(TypeSupport, Clone, Debug, PartialEq)]
#[dust_dds(nested, extensibility = "appendable")]
pub struct ProductInfo {
    pub nsn: String,
    pub gtin: String,
    pub manufacturer_name: String,
    pub model_name: String,
    pub model_number: String,
    pub serial_number: String,
    pub software_version: String,
    #[dust_dds(optional)]
    pub platform_id: Option<String>,
}

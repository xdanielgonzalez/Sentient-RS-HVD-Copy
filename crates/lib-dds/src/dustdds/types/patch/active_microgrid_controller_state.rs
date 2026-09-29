use dust_dds::infrastructure::type_support::TypeSupport;

/// dust_dds patch of `tms::ActiveMicrogridControllerState` (topic `ActiveMicrogridControllerState`) —
/// `@topic @extensibility(APPENDABLE)` (idl:4539). `device_id` is the instance `@key`. `master_id`
/// is `@optional string` → `Option<String>` + `#[dust_dds(optional)]` (wp-derive; the XCDR2
/// serializer emits a 1-byte presence flag before it in declared order, wp-serializer; the
/// deserializer mirrors it, wp-deserializer). Field order = the IDL member order, matching
/// `lib_tms::generated::ActiveMicrogridControllerState`. Derive-only: no hand impl, so this is a
/// single-type file by design.
#[derive(TypeSupport, Clone, Debug, PartialEq)]
#[dust_dds(extensibility = "appendable")]
pub struct ActiveMicrogridControllerState {
    #[dust_dds(key)]
    pub device_id: String,
    #[dust_dds(optional)]
    pub master_id: Option<String>,
}

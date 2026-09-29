use dust_dds::infrastructure::type_support::TypeSupport;

/// dust_dds patch of `tms::PowerPortId` — `@nested @extensibility(APPENDABLE) struct { Identity
/// deviceId; PowerPortNumber portNumber; }` (mil-std-3071_data_model.idl:2620), member-only.
/// Shared nested type: `PowerPortStatus.connection` is `@optional PowerPortId`, and it also appears
/// as `port_a`/`port_b` elsewhere. `device_id` is `String` (not `heapless::String` — dust_dds's
/// `DataStorageMapping`/`XTypesBinding` cover `alloc::String`/`Vec`/primitives only; the heapless
/// bound is a `lib_tms`-internal concern, not a wire one). Field order = the IDL member order,
/// matching `lib_tms::generated::PowerPortId`. Derive-only: no hand impl, so this is a single-type
/// file by design.
#[derive(TypeSupport, Clone, Debug, PartialEq)]
#[dust_dds(nested, extensibility = "appendable")]
pub struct PowerPortId {
    pub device_id: String,
    pub port_number: i32,
}

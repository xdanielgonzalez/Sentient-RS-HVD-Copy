use dust_dds::infrastructure::type_support::TypeSupport;

use super::power_port_status::PowerPortStatus;

/// dust_dds patch of `tms::PowerPortState` (topic `PowerPortState`) — `@topic
/// @extensibility(APPENDABLE)` (idl:3567). `device_id` is the instance `@key`; `power_ports` is a
/// `sequence<PowerPortStatus>` (rule-(12) NEXTINT delimiter on the wire; vendored dust_dds patch,
/// `vendor/dust_dds/PATCHES.md`). PowerPortState itself is `@optional`-free, but its element type
/// `PowerPortStatus` carries three `@optional` members, so the sequence element serialization
/// recurses through the optional-aware XCDR2 member walk (per-optional 1-byte presence flag,
/// wp-serializer/wp-deserializer). Field order = the IDL member order, matching
/// `lib_tms::generated::PowerPortState`. Derive-only: no hand impl, so this is a single-type file by
/// design.
#[derive(TypeSupport, Clone, Debug, PartialEq)]
#[dust_dds(extensibility = "appendable")]
pub struct PowerPortState {
    #[dust_dds(key)]
    pub device_id: String,
    pub power_ports: Vec<PowerPortStatus>,
}

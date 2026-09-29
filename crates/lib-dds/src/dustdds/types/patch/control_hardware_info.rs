use dust_dds::infrastructure::type_support::TypeSupport;

/// dust_dds patch of `tms::ControlHardwareInfo` — `@nested @extensibility(APPENDABLE)` (idl:2183),
/// member-only (`DeviceInfo.control_hardware`, itself `@optional`). All four members required. Field
/// order = the IDL member order, matching `lib_tms::generated::ControlHardwareInfo`. Derive-only: no
/// hand impl, so this is a single-type file by design.
#[derive(TypeSupport, Clone, Copy, Debug, PartialEq)]
#[dust_dds(nested, extensibility = "appendable")]
pub struct ControlHardwareInfo {
    pub has_realtime_clock: bool,
    pub min_temperature: f32,
    pub max_temperature: f32,
    pub num_network_ports: u16,
}

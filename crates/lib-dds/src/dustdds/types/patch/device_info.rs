use dust_dds::infrastructure::type_support::TypeSupport;

use super::control_hardware_info::ControlHardwareInfo;
use super::control_service_info::ControlServiceInfo;
use super::device_role::DeviceRole;
use super::parameter_metadata::ParameterMetadata;
use super::power_device_info::PowerDeviceInfo;
use super::power_hardware_info::PowerHardwareInfo;
use super::product_info::ProductInfo;
use super::topic_info::TopicInfo;

/// dust_dds patch of `tms::DeviceInfo` (topic `DeviceInfo`) — `@topic @extensibility(APPENDABLE)`
/// (idl:5286). The deepest patched topic: its transitive closure is ~30 nested types (all authored
/// under `patch/` by this WP, plus the pre-existing shared `Curve2D`/`Point2D`). `device_id` is the
/// instance `@key`. Six members are `@optional` → `Option<T>` + `#[dust_dds(optional)]` (wp-derive;
/// the XCDR2 serializer emits a 1-byte presence flag before each optional in declared order,
/// wp-serializer; the deserializer mirrors it, wp-deserializer). `control_parameters`/
/// `metric_parameters` are `@optional sequence<ParameterMetadata>` → `Option<Vec<ParameterMetadata>>`
/// (the chain uses `emit_optional_seq` → boolean-then-sequence, inner NEXTINT when present).
/// `role`/`product`/`topics` are required. Field order = the IDL member order, matching
/// `lib_tms::generated::DeviceInfo`. Derive-only: no hand impl, so this is a single-type file by
/// design.
#[derive(TypeSupport, Clone, Debug, PartialEq)]
#[dust_dds(extensibility = "appendable")]
pub struct DeviceInfo {
    #[dust_dds(key)]
    pub device_id: String,
    pub role: DeviceRole,
    pub product: ProductInfo,
    pub topics: TopicInfo,
    #[dust_dds(optional)]
    pub control_hardware: Option<ControlHardwareInfo>,
    #[dust_dds(optional)]
    pub power_hardware: Option<PowerHardwareInfo>,
    #[dust_dds(optional)]
    pub control_parameters: Option<Vec<ParameterMetadata>>,
    #[dust_dds(optional)]
    pub metric_parameters: Option<Vec<ParameterMetadata>>,
    #[dust_dds(optional)]
    pub control_service: Option<ControlServiceInfo>,
    #[dust_dds(optional)]
    pub power_device: Option<PowerDeviceInfo>,
}

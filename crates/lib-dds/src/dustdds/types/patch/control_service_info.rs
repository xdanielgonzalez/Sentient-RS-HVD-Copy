use dust_dds::infrastructure::type_support::TypeSupport;

use super::microgrid_controller_info::MicrogridControllerInfo;
use super::microgrid_dashboard_info::MicrogridDashboardInfo;

/// dust_dds patch of `tms::ControlServiceInfo` — `@nested @extensibility(APPENDABLE)` (idl:5212),
/// member-only (`DeviceInfo.control_service`, itself `@optional`). Both members (`mc`, `md`) are
/// `@optional` nested structs → `Option<T>` + `#[dust_dds(optional)]` (wp-derive; XCDR2 presence flag
/// from wp-serializer). Field order = the IDL member order, matching
/// `lib_tms::generated::ControlServiceInfo`. Derive-only: no hand impl, so this is a single-type file
/// by design.
#[derive(TypeSupport, Clone, Debug, PartialEq)]
#[dust_dds(nested, extensibility = "appendable")]
pub struct ControlServiceInfo {
    #[dust_dds(optional)]
    pub mc: Option<MicrogridControllerInfo>,
    #[dust_dds(optional)]
    pub md: Option<MicrogridDashboardInfo>,
}

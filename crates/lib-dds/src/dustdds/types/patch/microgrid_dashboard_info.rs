use dust_dds::infrastructure::type_support::TypeSupport;

use super::microgrid_dashboard_feature::MicrogridDashboardFeature;

/// dust_dds patch of `tms::MicrogridDashboardInfo` — `@nested @extensibility(APPENDABLE)` (idl:4330),
/// member-only (`ControlServiceInfo.md`, itself `@optional`). `features` is a
/// `sequence<MicrogridDashboardFeature>` — a non-primitive (enum) element sequence carrying the
/// rule-(12) NEXTINT delimiter (vendored dust_dds patch; `vendor/dust_dds/PATCHES.md`). Field order =
/// the IDL member order, matching `lib_tms::generated::MicrogridDashboardInfo`. Derive-only: no hand
/// impl, so this is a single-type file by design.
#[derive(TypeSupport, Clone, Debug, PartialEq)]
#[dust_dds(nested, extensibility = "appendable")]
pub struct MicrogridDashboardInfo {
    pub features: Vec<MicrogridDashboardFeature>,
}

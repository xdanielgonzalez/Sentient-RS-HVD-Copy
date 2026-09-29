use dust_dds::infrastructure::type_support::TypeSupport;

/// dust_dds patch of `tms::DeviceRole` — a fieldless enum (`@extensibility(APPENDABLE) enum
/// DeviceRole`), member-only (`DeviceInfo.role`). Variant order = the IDL declaration order (0-based
/// INT32 discriminants), matching `lib_tms::generated::DeviceRole` and the chains' `enum_u32`
/// encoding. Like the other enum patch types (`reply_code`/`config_id`/`dtc_severity`) this carries
/// only `#[dust_dds(nested)]` — the enum discriminator wire form is extensibility-independent.
/// Derive-only: no hand impl, so this is a single-type file by design.
#[derive(TypeSupport, Clone, Copy, Debug, PartialEq, Eq)]
#[dust_dds(nested)]
pub enum DeviceRole {
    Unknown,
    MicrogridController,
    Source,
    Load,
    Storage,
    Distribution,
    MicrogridDashboard,
    Conversion,
    Monitor,
}

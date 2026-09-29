use dust_dds::infrastructure::type_support::TypeSupport;

/// dust_dds patch of `tms::EnergyStartStopLevel` — a fieldless enum, member-only
/// (`SourceInfo`/`StorageInfo.supported_energy_start_stop_levels`). Variant order = the IDL
/// declaration order (0-based INT32 discriminants), matching `lib_tms::generated::EnergyStartStopLevel`.
/// Enum patch convention: `#[dust_dds(nested)]` only. Derive-only: no hand impl, so this is a
/// single-type file by design.
#[derive(TypeSupport, Clone, Copy, Debug, PartialEq, Eq)]
#[dust_dds(nested)]
pub enum EnergyStartStopLevel {
    Unknown,
    Any,
    Off,
    Warm,
    Idle,
    Ready,
    ReadySynced,
    Operational,
}

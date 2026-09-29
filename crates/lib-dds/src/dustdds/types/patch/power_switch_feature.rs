use dust_dds::infrastructure::type_support::TypeSupport;

/// dust_dds patch of `tms::PowerSwitchFeature` — a fieldless enum, member-only
/// (`PowerSwitchInfo.features`, `GroundingInfo.control_switch_features`/`pulse_switch_features`).
/// Variant order = the IDL declaration order (0-based INT32 discriminants), matching
/// `lib_tms::generated::PowerSwitchFeature`. Enum patch convention: `#[dust_dds(nested)]` only.
/// Derive-only: no hand impl, so this is a single-type file by design.
#[derive(TypeSupport, Clone, Copy, Debug, PartialEq, Eq)]
#[dust_dds(nested)]
pub enum PowerSwitchFeature {
    Unknown,
    ManualOpen,
    ManualClose,
    AutoOpen,
    AutoClose,
    RequestOpen,
    RequestClose,
    Lock,
    Recloser,
    Breaker,
    Gfi,
    ArcFlash,
    Synchronizer,
    Surge,
    SwitchConditions,
}

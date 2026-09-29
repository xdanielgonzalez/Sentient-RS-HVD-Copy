use dust_dds::infrastructure::type_support::TypeSupport;

/// dust_dds patch of `tms::PowerSwitchTransitionCode` — a fieldless enum (`@extensibility(APPENDABLE)
/// enum PowerSwitchTransitionCode` mil-std-3071_data_model.idl:2808), member-only
/// (`PowerSwitchStatus.future_continuity_reason` and `EnergyStartStopState.future_level_reason`,
/// both required). Variant order = the IDL declaration order (0-based INT32 discriminants), matching
/// `lib_tms::generated::PowerSwitchTransitionCode`. Enum patch convention: `#[dust_dds(nested)]`
/// only — the enum discriminator wire form is extensibility-independent. Derive-only: no hand impl,
/// so this is a single-type file by design.
///
/// Shared across the `PowerPortState` (via `PowerSwitchStatus`) and `EnergyStartStopState` closures;
/// authored here by wp-topic-small5. Not in the original WP file list — flagged for wp-patch-mod
/// wiring.
#[derive(TypeSupport, Clone, Copy, Debug, PartialEq, Eq)]
#[dust_dds(nested)]
pub enum PowerSwitchTransitionCode {
    Unknown,
    None,
    InProgress,
    PendingSync,
    PendingSwitchConditions,
    PendingInterlock,
    Complete,
    ManualOverride,
    AutoOverride,
    AutoLock,
    Malfunction,
}

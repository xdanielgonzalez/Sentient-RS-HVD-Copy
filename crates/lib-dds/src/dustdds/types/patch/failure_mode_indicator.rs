use dust_dds::infrastructure::type_support::TypeSupport;

/// dust_dds patch of `tms::FailureModeIndicator` — a fieldless enum (`@extensibility(APPENDABLE) enum
/// FailureModeIndicator` mil-std-3071_data_model.idl:869, "See SAE J1939"), member-only
/// (`DiagnosticTroubleCode.failure_mode`, a required member). Variant order = the IDL declaration
/// order (0-based INT32 discriminants), matching `lib_tms::generated::FailureModeIndicator`. Enum
/// patch convention: `#[dust_dds(nested)]` only — the enum discriminator wire form is
/// extensibility-independent. Derive-only: no hand impl, so this is a single-type file by design.
///
/// Exclusive to the `ActiveDiagnosticState` closure (via `DiagnosticTroubleCode`); authored here by
/// wp-topic-small5. Not in the original WP file list — flagged for wp-patch-mod wiring.
#[derive(TypeSupport, Clone, Copy, Debug, PartialEq, Eq)]
#[dust_dds(nested)]
pub enum FailureModeIndicator {
    Unknown,
    AboveNormalMostSevere,
    BelowNormalMostSevere,
    DataErratic,
    VoltageHigh,
    VoltageLow,
    CurrentLow,
    CurrentHigh,
    NotResponding,
    AbnormalFrequency,
    AbnormalUpdateRate,
    AbnormalRateOfChange,
    FailureCodeNotIdentifiable,
    BadIntelligentDevice,
    OutOfCalibration,
    SpecialInstructions,
    AboveNormalLeastSevere,
    AboveNormalModeratelySevere,
    BelowNormalLeastSevere,
    BelowNormalModeratelySevere,
    ReceivedDataError,
    DataDriftedHigh,
    DataDriftedLow,
    ConditionExists,
}

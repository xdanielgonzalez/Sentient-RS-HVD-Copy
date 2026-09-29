use dust_dds::infrastructure::type_support::TypeSupport;

/// dust_dds patch of `tms::DtcSeverity` — a fieldless enum (`@extensibility(APPENDABLE) enum
/// DtcSeverity` mil-std-3071_data_model.idl:498), member-only (never a topic in its own right).
/// Shared nested type: `DiagnosticTroubleCode.estimate_severity` is `@optional DtcSeverity`, and
/// the required `severity` member uses it too. Variant order = the IDL declaration order (0-based
/// INT32 discriminants), matching `lib_tms::generated::DtcSeverity` and the chains' `enum_u32`
/// encoding. Like the other enum patch types (`reply_code`/`config_id`) this carries only
/// `#[dust_dds(nested)]` — the enum discriminator wire form is extensibility-independent. Derive-
/// only: no hand impl, so this is a single-type file by design.
#[derive(TypeSupport, Clone, Copy, Debug, PartialEq, Eq)]
#[dust_dds(nested)]
pub enum DtcSeverity {
    Unknown,
    Clear,
    Informative,
    Preventative,
    Degraded,
    Warning,
    Minor,
    Major,
    Critical,
    Fatal,
    HumanSafety,
}

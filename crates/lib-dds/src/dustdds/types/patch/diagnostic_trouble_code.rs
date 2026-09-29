use dust_dds::infrastructure::type_support::TypeSupport;

use super::clock_monotonic::ClockMonotonic;
use super::dtc_severity::DtcSeverity;
use super::failure_mode_indicator::FailureModeIndicator;

/// dust_dds patch of `tms::DiagnosticTroubleCode` — `@nested @extensibility(APPENDABLE)` (idl:2416),
/// member-only (`ActiveDiagnosticState.codes`, a `sequence<DiagnosticTroubleCode>`). Three members
/// are `@optional` → `Option<T>` + `#[dust_dds(optional)]` (wp-derive; the XCDR2 serializer emits a
/// 1-byte presence flag before each in declared order, wp-serializer; the deserializer mirrors it,
/// wp-deserializer): `port_number` (`Option<i32>`), `estimate_time` (`Option<ClockMonotonic>`),
/// `estimate_severity` (`Option<DtcSeverity>`). The `@optional port_number` is the FIRST member, so
/// its presence flag leads the struct's XCDR2 body. The other members are required. Field order =
/// the IDL member order, matching `lib_tms::generated::DiagnosticTroubleCode`. Derive-only: no hand
/// impl, so this is a single-type file by design.
///
/// Exclusive to the `ActiveDiagnosticState` closure; authored here by wp-topic-small5.
#[derive(TypeSupport, Clone, Debug, PartialEq)]
#[dust_dds(nested, extensibility = "appendable")]
pub struct DiagnosticTroubleCode {
    #[dust_dds(optional)]
    pub port_number: Option<i32>,
    pub suspect_parameter: u32,
    pub failure_mode: FailureModeIndicator,
    pub occurrence_count: u32,
    pub time_raised: ClockMonotonic,
    pub severity: DtcSeverity,
    #[dust_dds(optional)]
    pub estimate_time: Option<ClockMonotonic>,
    #[dust_dds(optional)]
    pub estimate_severity: Option<DtcSeverity>,
    pub hint: String,
}

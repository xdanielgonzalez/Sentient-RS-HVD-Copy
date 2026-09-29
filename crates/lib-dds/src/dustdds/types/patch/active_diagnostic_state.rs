use dust_dds::infrastructure::type_support::TypeSupport;

use super::diagnostic_trouble_code::DiagnosticTroubleCode;

/// dust_dds patch of `tms::ActiveDiagnosticState` (topic `ActiveDiagnosticState`) — `@topic
/// @extensibility(APPENDABLE)` (idl:3215). `device_id` is the instance `@key`; `codes` is a
/// `sequence<DiagnosticTroubleCode>` (rule-(12) NEXTINT delimiter on the wire; vendored dust_dds
/// patch, `vendor/dust_dds/PATCHES.md`); `overflow` is a required `uint32`. ActiveDiagnosticState
/// itself is `@optional`-free, but its element type `DiagnosticTroubleCode` carries three `@optional`
/// members, so the sequence element serialization recurses through the optional-aware XCDR2 member
/// walk (per-optional 1-byte presence flag, wp-serializer/wp-deserializer). Field order = the IDL
/// member order, matching `lib_tms::generated::ActiveDiagnosticState`. Derive-only: no hand impl, so
/// this is a single-type file by design.
#[derive(TypeSupport, Clone, Debug, PartialEq)]
#[dust_dds(extensibility = "appendable")]
pub struct ActiveDiagnosticState {
    #[dust_dds(key)]
    pub device_id: String,
    pub codes: Vec<DiagnosticTroubleCode>,
    pub overflow: u32,
}

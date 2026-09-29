use dust_dds::infrastructure::type_support::TypeSupport;

use super::curve2d::Curve2D;

/// dust_dds patch of `tms::ControlCurve` — `{ Curve2D minimum; Curve2D nominal; Curve2D maximum; }`,
/// member-only (nested inside `DcLoadSharingParameters.reference_voltage`). Field order = the IDL
/// member order. Derive-only: no hand impl, so this is a single-type file by design.
#[derive(TypeSupport, Clone, Debug, PartialEq)]
#[dust_dds(nested, extensibility = "appendable")]
pub struct ControlCurve {
    pub minimum: Curve2D,
    pub nominal: Curve2D,
    pub maximum: Curve2D,
}

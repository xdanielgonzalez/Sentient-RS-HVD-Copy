use dust_dds::infrastructure::type_support::TypeSupport;

/// dust_dds patch of `tms::Point2D` — `{ float x; float y; }`, member-only (nested inside
/// `Curve2D.points`). Field order = the IDL member order. Derive-only: no hand impl, so this is a
/// single-type file by design.
#[derive(TypeSupport, Clone, Copy, Debug, PartialEq)]
#[dust_dds(nested, extensibility = "appendable")]
pub struct Point2D {
    pub x: f32,
    pub y: f32,
}

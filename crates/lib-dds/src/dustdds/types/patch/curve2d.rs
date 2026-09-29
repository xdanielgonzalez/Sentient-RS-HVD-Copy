use dust_dds::infrastructure::type_support::TypeSupport;

use super::point2d::Point2D;

/// dust_dds patch of `tms::Curve2D` — `{ sequence<Point2D> points; }`, member-only (nested inside
/// `ControlCurve`). `points` is `Vec<T>` (not `heapless::Vec`) — dust_dds's `XTypesBinding` blanket
/// only covers `alloc::vec::Vec<T: TypeSupport>` (this crate is Pi-only std, so the heapless bound
/// is a `lib_tms`-internal concern, not a wire one). This is a NON-primitive-element sequence, so on
/// the wire it carries the rule-(12) NEXTINT delimiter (vendored dust_dds patch;
/// `vendor/dust_dds/PATCHES.md`). Derive-only: no hand impl, so this is a single-type file by design.
#[derive(TypeSupport, Clone, Debug, PartialEq)]
#[dust_dds(nested, extensibility = "appendable")]
pub struct Curve2D {
    pub points: Vec<Point2D>,
}

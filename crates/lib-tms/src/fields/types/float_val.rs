use super::fixed_num::FixedNum;

/// A numeric field value, tagged by which [`super::json_sink::JsonSink`] method emits it — the
/// argument type for [`crate::fields::emit_num`]. `Fixed` covers the canonical-integer scaled-decimal
/// path (the common case); `F32`/`F64` cover fields that are genuinely floating-point at the source
/// (no canonical scaled-integer representation exists); `Null` covers fields with no data at all
/// (e.g. the permanently-absent TMS fields), emitted directly with no numeric value constructed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FloatVal {
    Fixed(FixedNum),
    F32(f32),
    F64(f64),
    Null,
}

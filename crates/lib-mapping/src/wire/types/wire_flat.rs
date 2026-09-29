use super::wire_error::WireError;
use lib_tms::fields::JsonSink;

/// A flat, NON-recursive, no-alloc TMS-JSON encoder — the T3-safe hot path.
///
/// The MCU never materializes the reflective/recursive TMS struct tree (that recursion is exactly
/// what blew the broker-thread stack — the "T3" incident, see `lib-mapping::wire`'s module doc and
/// `CLAUDE.md`). Instead, the generator emits ONE `impl WireFlat` per MCU wire topic whose body is
/// straight-line `sink.str(..)`/`sink.fixed(..)` calls plus at most one bounded `for` loop — the
/// exact shape (and byte output) of the hand-written `lib_mapping::wire::serialize_*` functions.
///
/// `Self` is the per-topic *wire binding* (e.g. `DcMeasurementUpdateWire`), which borrows the
/// canonical device model (`CanonicalReading`/`CanonicalFault`) + device context (`Identity`,
/// `ClockMonotonic`). It is NOT the materialized f32-carrying TMS struct: the golden serializer
/// renders scaled canonical integers as fixed-point decimals (`5452` → `"54.52"`), which a generic
/// walk over the f32 struct (ryu shortest-round-trip) could not reproduce byte-for-byte. Carrying
/// the canonical bindings is what preserves byte-identity — proven in
/// `crates/lib-tms-model/tests/wire_byte_identity.rs`.
pub trait WireFlat {
    /// Write this topic's TMS-JSON flatly into `sink`, byte-identically to the hand-written
    /// `lib_mapping::wire::serialize_*` for the same inputs.
    fn serialize_flat(&self, sink: &mut impl JsonSink) -> Result<(), WireError>;
}

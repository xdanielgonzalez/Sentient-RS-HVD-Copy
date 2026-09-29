/// Error returned by [`super::wire_flat::WireFlat::serialize_flat`]. The flat encoders are
/// straight-line writes into a caller-owned [`lib_tms::fields::JsonSink`] (which itself absorbs
/// buffer overflow via its own `overflowed()` flag), so the encode step is INFALLIBLE today — this
/// is an uninhabited placeholder that keeps the trait's return type honest for a future fallible
/// encoder without forcing a breaking signature change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireError {}

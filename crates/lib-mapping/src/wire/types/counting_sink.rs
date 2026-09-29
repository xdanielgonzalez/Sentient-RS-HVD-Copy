//! [`CountingSink`] — the length-only [`crate::wire::JsonSink`] impl. See
//! [`lib_tms::fields::JsonSink`]'s doc comment for why this exists (one-pass MQTT PUBLISH length,
//! guaranteed never to desync from [`crate::wire::WriteSink`]'s emitted bytes).

/// A [`crate::wire::JsonSink`] that emits nothing and only accumulates the byte count each method
/// call *would* have written. Used by [`crate::wire::serialized_len`] to compute the MQTT PUBLISH
/// payload length before the header is written, from the exact same serialize function that then
/// streams the real bytes via [`crate::wire::WriteSink`].
#[derive(Debug, Default, Clone, Copy)]
pub struct CountingSink {
    pub(crate) len: usize,
}

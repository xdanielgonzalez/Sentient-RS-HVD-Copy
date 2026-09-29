use super::prelude::*;

/// Render `value`'s complete RTPS XCDR2 SerializedPayload via its `lib_tms` chain — identical to
/// `lib_mapping::render_cdr::render_cdr_value` (replicated here so this bin needs only `lib_tms`,
/// not the full mapping pipeline — kept minimal per the WP5 brief). Shared by `dump-fixtures` and
/// `publish` mode so there is exactly one render path in this bin.
pub(crate) fn render<T: HasCdrChain>(v: &T) -> Vec<u8> {
    let mut buf = [0u8; 4096];
    let mut sink = CdrWriteSink::new(&mut buf);
    emit_object_cdr::<T, T::CdrChain, _>(v, &mut sink);
    sink.finalize_encapsulation_padding();
    let n = sink.len();
    buf[..n].to_vec()
}

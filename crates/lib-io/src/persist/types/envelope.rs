use serde_json::value::RawValue;

/// The on-disk JSONL envelope: `{"ts":..,"bus":"..","topic":"..","payload":<verbatim JSON>}`. Only
/// ever constructed to be immediately serialized (see `impl_jsonl_writer.rs`'s `format_line`) —
/// `payload` is a [`RawValue`] so the already-rendered TMS JSON is embedded byte-for-byte, never
/// re-escaped as a JSON string. Internal to this crate.
#[derive(serde::Serialize)]
pub(crate) struct Envelope<'a> {
    pub(crate) ts: u64,
    pub(crate) bus: &'a str,
    pub(crate) topic: &'a str,
    pub(crate) payload: &'a RawValue,
}

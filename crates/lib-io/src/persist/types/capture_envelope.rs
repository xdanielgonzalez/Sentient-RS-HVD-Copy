/// The on-disk raw-capture JSONL envelope:
/// `{"ts":..,"bus":"..","request":"<hex>","response":"<hex>","valid":..}`. Hex (not base64) so the
/// `(`/`)`-framed ASCII Voltronic/Seplos wire bytes stay grep-able straight out of the raw file —
/// see `impl_capture_writer.rs`'s `format_capture_line`, the only place this is constructed.
/// Internal to this crate.
#[derive(serde::Serialize)]
pub(crate) struct CaptureEnvelope<'a> {
    pub(crate) ts: u64,
    pub(crate) bus: &'a str,
    pub(crate) request: &'a str,
    pub(crate) response: &'a str,
    pub(crate) valid: bool,
}

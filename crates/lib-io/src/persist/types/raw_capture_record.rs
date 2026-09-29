/// One raw pre-parse wire exchange (request written + response read), captured for field
/// diagnostics of the real Voltronic/Seplos wire protocol — decision 0001's open risk ("this
/// environment had no real Voltronic/QPIGS wire captures... must be verified against real
/// captures... before an inverter deployment is trusted"). Persisted verbatim via
/// [`crate::persist::run_capture_writer`], independent of whether `validate_frame`/parsing succeeded on it
/// — `valid` records that outcome instead, so a garbled/rejected frame is still captured rather
/// than dropped.
///
/// Distinct from [`super::log_record::LogRecord`]: `LogRecord` tails already-rendered/parsed TMS
/// JSON (Scope A); this tails the pre-parse wire bytes themselves.
#[derive(Debug, Clone)]
pub struct RawCaptureRecord {
    /// Wall-clock milliseconds at the time this exchange was captured.
    pub ts_ms: u64,
    /// Which bus/interface this capture originated from (multi-port future — see `LogRecord::bus`
    /// for the same convention). A single-port deployment always uses the same value.
    pub bus: String,
    /// Raw bytes written to the wire (the request/command frame), verbatim.
    pub request: Vec<u8>,
    /// Raw bytes read back from the wire (the response), verbatim — may be a truncated/garbled
    /// frame if the transport errored or frame validation failed (see `valid`).
    pub response: Vec<u8>,
    /// Whether `validate_frame`/parsing succeeded on this exchange, so a field engineer can filter
    /// straight to the frames worth investigating.
    pub valid: bool,
}

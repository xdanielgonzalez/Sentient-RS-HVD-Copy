/// One tailed telemetry/publish record, appended to the jsonl log as one line.
#[derive(Debug, Clone)]
pub struct LogRecord {
    /// Wall-clock milliseconds at the time this record was offered to the writer.
    pub ts_ms: u64,
    /// Which bus/interface this record originated from (multi-port future — see the gap-analysis
    /// "Multi-port serial" finding). A single-port deployment always uses the same value.
    pub bus: String,
    /// The MQTT topic this record was (or would be) published on.
    pub topic: String,
    /// The rendered payload bytes (JSON), verbatim as published.
    pub payload: Vec<u8>,
}

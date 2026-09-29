/// Parsed `--scan-bus` CLI flags (see the `crate::scan` module doc for the full usage string).
///
/// [`ScanArgs::parse`] returns `None` when `--scan-bus` itself is absent — that is the "is this
/// scan mode at all" check. Every other field is optional and falls back to the loaded
/// `crate::config::Config` (when available) in [`ScanArgs::resolve`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScanArgs {
    /// `--port <path>`. Falls back to the first configured bus's serial port when absent.
    pub port: Option<String>,
    /// `--baud <n>`. Falls back to the resolved bus's configured baud, else the Seplos default
    /// (19200) when no config is available.
    pub baud: Option<u32>,
    /// `--timeout-ms <n>`. Defaults to `crate::scan::DEFAULT_SCAN_TIMEOUT_MS` (200 ms) — always,
    /// regardless of the resolved bus's own (longer) production `read_timeout_ms` — see
    /// [`ScanArgs::resolve`]'s doc comment.
    pub timeout_ms: Option<u32>,
}

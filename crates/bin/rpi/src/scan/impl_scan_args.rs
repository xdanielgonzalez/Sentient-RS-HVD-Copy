use super::prelude::*;

impl ScanArgs {
    /// Parse `--scan-bus [--port P] [--baud N] [--timeout-ms N]` out of raw CLI args. Returns
    /// `None` when `--scan-bus` itself is absent — the "is this scan mode at all" check a caller
    /// (`main.rs`'s dispatcher — see the `crate::scan` module doc for the integration TODO) runs
    /// first; every other flag is optional and falls back to the loaded config in
    /// [`ScanArgs::resolve`].
    pub fn parse(args: &[String]) -> Option<ScanArgs> {
        if !args.iter().any(|a| a == "--scan-bus") {
            return None;
        }
        Some(ScanArgs {
            port: flag_value(args, "--port").map(str::to_string),
            baud: flag_value(args, "--baud").and_then(|v| v.parse().ok()),
            timeout_ms: flag_value(args, "--timeout-ms").and_then(|v| v.parse().ok()),
        })
    }

    /// Resolve the concrete port/kind/params/timeout to scan: an explicit flag always wins; an
    /// unset field falls back to the first bus in `cfg` (if any and if `cfg.buses()` itself parses
    /// cleanly); `port` has no further fallback beyond that (a hard error — a scan needs a device
    /// path from somewhere), `baud`/`kind`/the other line params fall back to
    /// [`SerialParams::default`]'s Seplos defaults (19200 8N1) and [`SerialKind::Rs485`] (this
    /// tool's whole purpose — a multi-drop RS-485 DIP bus) when no config is available.
    ///
    /// `timeout_ms` ALWAYS defaults to `crate::scan::DEFAULT_SCAN_TIMEOUT_MS` (200 ms) unless
    /// `--timeout-ms` is given explicitly — deliberately NOT the resolved bus's own, longer,
    /// production `read_timeout_ms`: a diagnostic sweep across 16 addresses wants a fast per-address
    /// deadline (most addresses are expected to be silent), not the per-cycle production one.
    pub fn resolve(&self, cfg: Option<&Config>) -> anyhow::Result<ResolvedBus> {
        let bus = cfg.and_then(|c| match c.buses() {
            Ok(buses) => {
                if buses.len() > 1 {
                    log::warn!(
                        "--scan-bus: {} buses configured; scanning the first ({}) — pass --port \
                         to target a different one",
                        buses.len(),
                        buses.first().map_or("?", |b| b.name.as_str())
                    );
                }
                buses.into_iter().next()
            }
            Err(e) => {
                log::warn!(
                    "--scan-bus: config present but its bus section is invalid ({e}); \
                     proceeding with only the explicit --port/--baud flags given"
                );
                None
            }
        });

        let port = self
            .port
            .clone()
            .or_else(|| bus.as_ref().map(|b| b.serial.port.clone()))
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "--scan-bus: no --port given and no configured serial bus found; pass \
                     --port explicitly or point SENTIENT_CONFIG at a config with [serial] or \
                     [[bus]]"
                )
            })?;

        let kind = bus
            .as_ref()
            .map_or(SerialKind::Rs485, |b| b.serial.serial_kind());
        let mut params = bus
            .as_ref()
            .map_or_else(SerialParams::default, |b| b.serial.serial_params());
        if let Some(baud) = self.baud {
            params.baud = baud;
        }
        let timeout_ms = self.timeout_ms.unwrap_or(super::DEFAULT_SCAN_TIMEOUT_MS);

        Ok(ResolvedBus {
            port,
            kind,
            params,
            timeout_ms,
        })
    }
}

/// Look up `--flag VALUE` in raw CLI args, mirroring `main.rs`'s own manual arg-parsing style
/// (this crate wires no `clap` dependency).
fn flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let idx = args.iter().position(|a| a == flag)?;
    args.get(idx + 1).map(String::as_str)
}

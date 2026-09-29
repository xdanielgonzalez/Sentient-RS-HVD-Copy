//! Standalone RS-485 DIP-address bus scanner — the legacy AWS pipeline's field diagnostic tool
//! (walks all 16 DIP addresses to find live units), reimplemented here. See
//! `docs/gap-analysis-meteor-parity.md`'s "Standalone RS-485 DIP-address bus scanner" finding
//! (issue #243, part of the gap-closure umbrella #237).
//!
//! # Usage
//!
//! ```text
//! bin-rpi --scan-bus [--port /dev/ttySC0] [--baud N] [--timeout-ms 200]
//! ```
//!
//! * `--scan-bus` — enter scan mode; this flag is the whole reason this module runs at all. Every
//!   other flag below is optional.
//! * `--port <path>` — the serial device to scan. When absent, the first bus in the loaded config
//!   (the legacy `[serial]` section, or the first `[[bus]]` entry) supplies it — see
//!   [`ScanArgs::resolve`]. A config with no serial bus AND no `--port` is a hard error: a scan
//!   needs a device path from somewhere.
//! * `--baud <n>` — line baud rate. Falls back to the resolved bus's configured baud, else the
//!   Seplos default (19200) when no config is available.
//! * `--timeout-ms <n>` — per-address read deadline. Defaults to [`DEFAULT_SCAN_TIMEOUT_MS`]
//!   (200 ms) regardless of the resolved bus's own, longer, production `read_timeout_ms` — a
//!   sweep across 16 addresses wants a fast walk (most are expected to be silent), not the
//!   per-cycle production deadline.
//!
//! Walks every DIP address `DIP_MIN..=DIP_MAX` (`0x00..=0x0F`) on the resolved port via the same
//! `lib_io::driver::read_telemetry` exchange + `lib_parse::seplos::Seplos` vendor decode the
//! production poll loop uses ([`impl_app::run`]), printing one table row per address: LIVE (with
//! the decoded pack voltage in mV), SILENT (no response — the expected outcome for an
//! unpopulated DIP slot), or an ERROR detail (something answered but the exchange failed — bad
//! checksum, a wrong echoed address, a short/garbled frame).
//!
//! **Port contention**: this tool opens the serial port exclusively, the same way the `sentient`
//! service itself does. **Stop the `sentient` systemd service first**
//! (`systemctl stop sentient`) — scanning while the service holds the port will fail to open it
//! (or, worse, race the service's own request/response exchange on a shared bus). [`run`] logs a
//! reminder before it opens the port, and folds the same reminder into the error if the open
//! itself fails.
//!
//! Only the default RS-485 direction mode (software RTS toggling — see
//! `lib_io::host::HostSerial::open`) is used; a bus configured for `kernel`/`gpio` direction
//! control isn't (yet) read from `[serial].direction` here (that field is itself still a STUB
//! passthrough as of #245 — see `crate::config::SerialConfig`'s doc comment).
//!
//! # Integration
//!
//! Wired into `main.rs`'s dispatcher (#237 integration pass) alongside the
//! `--emit-mosquitto-bridge` early-exit branch, before `Config::load`/`runtime::supervise`:
//! ```text
//! if let Some(scan_args) = scan::ScanArgs::parse(&args) {
//!     return scan::run(&scan_args).await;
//! }
//! ```

mod impl_scan_args;
mod impl_scan_status;
mod prelude;
mod types;

pub use types::*;

use lib_io::driver::{read_telemetry, ExchangeError};
use lib_io::host::HostSerial;
use lib_io::transport::{IoError, SerialTransport};
use lib_parse::seplos::Seplos;

/// First DIP address walked.
pub const DIP_MIN: u8 = 0x00;
/// Last DIP address walked (inclusive) — 16 addresses total, the Seplos protocol's full DIP range.
pub const DIP_MAX: u8 = 0x0F;
/// `--timeout-ms` default — see the module doc's flag list for why this is deliberately shorter
/// than a bus's production `read_timeout_ms`.
pub const DEFAULT_SCAN_TIMEOUT_MS: u32 = 200;

/// Resolve `args` (against the loaded config, when its own fields are absent), open the port,
/// walk every DIP address, and print the live/silent/error table. The single entry point an
/// integration caller needs — see this module's doc comment for the `main.rs` wiring TODO.
pub async fn run(args: &ScanArgs) -> anyhow::Result<()> {
    let cfg = match crate::config::Config::load(crate::config_path().as_deref()) {
        Ok(c) => Some(c),
        Err(e) => {
            log::warn!(
                "--scan-bus: could not load a config to reuse ({e}); proceeding with only the \
                 explicit --port/--baud/--timeout-ms flags given"
            );
            None
        }
    };
    let bus = args.resolve(cfg.as_ref())?;

    log::warn!(
        "--scan-bus: opening {} exclusively — stop the sentient service first \
         (`systemctl stop sentient`) if this fails to open, or if a live scan and the service \
         would otherwise race each other on a shared bus",
        bus.port
    );
    let mut transport =
        HostSerial::open(&bus.port, bus.kind, bus.params, bus.timeout_ms).map_err(|e| {
            anyhow::anyhow!(
                "failed to open {}: {e} (if the sentient service is running, stop it first — \
                 `systemctl stop sentient` — before scanning; a serial port can only be held by \
                 one process at a time)",
                bus.port
            )
        })?;

    println!(
        "scanning {} ({:?}, {} baud, {} ms/address, DIP {DIP_MIN:#04x}..={DIP_MAX:#04x})",
        bus.port, bus.kind, bus.params.baud, bus.timeout_ms
    );
    let results = walk_bus(&mut transport).await;
    print_report(&results);

    Ok(())
}

/// Probe every DIP address `DIP_MIN..=DIP_MAX` in order over `transport`, one
/// `lib_io::driver::read_telemetry` exchange each — the exact decode path the production poll
/// loop ([`impl_app::run`]) uses. Generic over the transport so a test can drive it with
/// `lib_io::mock::MockSerial` instead of a real port.
async fn walk_bus<T: SerialTransport>(transport: &mut T) -> Vec<ScanResult> {
    let vendor = Seplos::new();
    let mut buf = [0u8; 256];
    let mut results = Vec::with_capacity((DIP_MAX - DIP_MIN) as usize + 1);
    for dip in DIP_MIN..=DIP_MAX {
        let (status, pack_mv) = match read_telemetry(transport, &vendor, dip, &mut buf).await {
            Ok(reading) => (ScanStatus::Live, Some(reading.total_voltage_cv as u32 * 10)),
            Err(ExchangeError::Io(IoError::Timeout)) => (ScanStatus::Silent, None),
            Err(e) => (ScanStatus::Error(format!("{e:?}")), None),
        };
        results.push(ScanResult {
            dip,
            status,
            pack_mv,
        });
    }
    results
}

/// Print the scan table: one row per address, then a live-count summary.
fn print_report(results: &[ScanResult]) {
    println!();
    println!("{:<6}{:<20}{:>10}", "DIP", "STATUS", "PACK_MV");
    for r in results {
        let dip = format!("{:#04x}", r.dip);
        let mv = r
            .pack_mv
            .map(|v| v.to_string())
            .unwrap_or_else(|| "-".to_string());
        println!("{dip:<6}{:<20}{mv:>10}", r.status.to_string());
    }
    let live = results
        .iter()
        .filter(|r| r.status == ScanStatus::Live)
        .count();
    println!();
    println!("{live} of {} address(es) live", results.len());
}

#[cfg(test)]
// Test code may unwrap/panic; the workspace denies these in logic only.
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use lib_io::mock::{MockSerial, MockStep};

    /// The canonical live 14-cell telemetry response, DIP 0 (same fixture as
    /// `lib-parse/tests/seplos_vectors.rs`'s `LIVE_FRAME`; `total_voltage_cv` = 5452 -> 54520 mV).
    const LIVE_FRAME: &[u8] = b"~20004600A08E00000E0F360F360F360F360F350F360F360F360F360F370F370F360F360F35060AF10AF90AF90AF30B370B0CFF5C154C36820A465003074650000203E8154CFFF800000079004DDEC8\r";

    #[tokio::test]
    async fn walk_bus_classifies_live_silent_and_error_addresses() {
        // DIP 0: a valid frame -> Live, pack_mv decoded. DIP 1: silent (Timeout). DIP 2: the SAME
        // frame replayed (its embedded address is 0, but the exchange addresses DIP 2) -> an
        // address mismatch -> Error. DIP 3..=15: silent.
        let mut steps: Vec<MockStep<'_>> = vec![
            MockStep::Reply(LIVE_FRAME),
            MockStep::Timeout,
            MockStep::Reply(LIVE_FRAME),
        ];
        steps.extend(std::iter::repeat_n(
            MockStep::Timeout,
            (DIP_MAX - 2) as usize,
        ));
        let mut mock = MockSerial::<32>::new(&steps);

        let results = walk_bus(&mut mock).await;

        assert_eq!(results.len(), 16);

        assert_eq!(results[0].dip, 0);
        assert_eq!(results[0].status, ScanStatus::Live);
        assert_eq!(results[0].pack_mv, Some(54_520));

        assert_eq!(results[1].dip, 1);
        assert_eq!(results[1].status, ScanStatus::Silent);
        assert_eq!(results[1].pack_mv, None);

        assert_eq!(results[2].dip, 2);
        assert!(matches!(results[2].status, ScanStatus::Error(_)));
        assert_eq!(results[2].pack_mv, None);

        for r in &results[3..] {
            assert_eq!(r.status, ScanStatus::Silent);
            assert_eq!(r.pack_mv, None);
        }
    }

    #[tokio::test]
    async fn walk_bus_reports_a_fully_silent_bus() {
        let steps = [MockStep::Timeout; 16];
        let mut mock = MockSerial::<32>::new(&steps);

        let results = walk_bus(&mut mock).await;

        assert_eq!(results.len(), 16);
        assert!(results.iter().all(|r| r.status == ScanStatus::Silent));
    }

    #[test]
    fn scan_args_parse_returns_none_without_scan_bus_flag() {
        let args: Vec<String> = vec!["--port".into(), "/dev/ttyUSB0".into()];
        assert!(ScanArgs::parse(&args).is_none());
    }

    #[test]
    fn scan_args_parse_reads_every_flag() {
        let args: Vec<String> = [
            "--scan-bus",
            "--port",
            "/dev/ttySC0",
            "--baud",
            "9600",
            "--timeout-ms",
            "50",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        let parsed = ScanArgs::parse(&args).expect("--scan-bus present");
        assert_eq!(parsed.port.as_deref(), Some("/dev/ttySC0"));
        assert_eq!(parsed.baud, Some(9600));
        assert_eq!(parsed.timeout_ms, Some(50));
    }

    #[test]
    fn scan_args_parse_defaults_every_optional_flag_to_none() {
        let args: Vec<String> = vec!["--scan-bus".into()];
        let parsed = ScanArgs::parse(&args).expect("--scan-bus present");
        assert_eq!(parsed, ScanArgs::default());
    }

    #[test]
    fn resolve_errors_without_port_or_config() {
        let args = ScanArgs::default();
        assert!(args.resolve(None).is_err());
    }

    #[test]
    fn resolve_uses_explicit_flags_with_no_config() {
        let args = ScanArgs {
            port: Some("/dev/ttyUSB9".to_string()),
            baud: Some(4800),
            timeout_ms: Some(75),
        };
        let bus = args
            .resolve(None)
            .expect("explicit port resolves without config");
        assert_eq!(bus.port, "/dev/ttyUSB9");
        assert_eq!(bus.params.baud, 4800);
        assert_eq!(bus.timeout_ms, 75);
        assert_eq!(bus.kind, lib_io::host::SerialKind::Rs485);
    }

    #[test]
    fn resolve_default_timeout_is_the_scan_default_not_the_production_one() {
        let args = ScanArgs {
            port: Some("/dev/ttyUSB9".to_string()),
            baud: None,
            timeout_ms: None,
        };
        let bus = args.resolve(None).expect("resolves");
        assert_eq!(bus.timeout_ms, DEFAULT_SCAN_TIMEOUT_MS);
        // The Seplos default (SerialParams::default()) when no config supplies one.
        assert_eq!(bus.params.baud, 19_200);
    }
}

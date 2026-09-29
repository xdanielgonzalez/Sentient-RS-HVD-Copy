//! `seplos-tcp-sim` — a std host binary that puts [`SeplosSim`] on the wire.
//!
//! Two jobs, both reusing `SeplosSim`/`BatteryState`/`Behavior` **verbatim** (never re-deriving
//! Seplos frames or checksums):
//!
//! 1. **TCP peer mode** (`--connect HOST:PORT` or `--listen PORT`): act as the Seplos RS-485
//!    device for the Renode e2e. Renode's `CreateServerSocketTerminal` makes Renode the TCP
//!    server for the emulated UART, so `--connect` (dialing in as a client) is the primary mode;
//!    `--listen` (binding/accepting) exists for local testing without Renode. A stdin control
//!    protocol lets a Robot Framework keyword change behavior/state/faults mid-run. Reply bytes
//!    are written instantly by default; `--pace <baud>` (e.g. `--pace 19200`, with an optional
//!    `--turnaround-ms <n>`) instead paces them at real RS-485 wire rate — see
//!    `impl_core_pacing` — so the e2e can run at "the rate of the real line" when that matters.
//! 2. **`--dump-fixtures <dir>` mode**: no networking at all — runs the full offline pipeline
//!    (`SeplosSim` → `lib_parse` → `lib_mapping` → JSON) to produce the golden TMS fixtures the
//!    accuracy/compliance Robot suites compare live MQTT payloads against. This guarantees
//!    byte/format identity with the firmware's own encoder (same `serde-json-core` codec),
//!    including the `NaN` → `null` policy.
//!
//! No `unwrap`/`expect`/`panic` in any path that can fail on I/O or bad input: errors are
//! propagated as `Result` and reported on stderr with a non-zero exit code.

mod impl_app_error;
mod impl_cli;
mod impl_core_connection;
mod impl_core_control_parse;
mod impl_core_fixtures;
mod impl_core_pacing;
mod impl_pace_config;
mod prelude;
mod types;

use impl_core_connection::serve_connection;
use impl_core_fixtures::dump_fixtures;
use prelude::*;

fn main() {
    if let Err(e) = run() {
        eprintln!("seplos-tcp-sim: error: {e}");
        std::process::exit(1);
    }
}

/// Top-level CLI dispatch. Kept out of `main` so every fallible step returns `Result` instead of
/// panicking.
fn run() -> Result<(), AppError> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cli = Cli::parse(&args)?;

    match cli.mode {
        Mode::DumpFixtures(dir) => dump_fixtures(&dir),
        Mode::Connect(addr) => {
            let stream = TcpStream::connect(&addr).map_err(|e| AppError::Io {
                what: format!("connect to {addr}"),
                source: e,
            })?;
            serve_connection(stream, cli.pace, cli.drift)
        }
        Mode::Listen(port) => {
            let listener = TcpListener::bind(("0.0.0.0", port)).map_err(|e| AppError::Io {
                what: format!("listen on port {port}"),
                source: e,
            })?;
            let (stream, peer) = listener.accept().map_err(|e| AppError::Io {
                what: "accept connection".to_string(),
                source: e,
            })?;
            eprintln!("seplos-tcp-sim: accepted connection from {peer}");
            serve_connection(stream, cli.pace, cli.drift)
        }
    }
}

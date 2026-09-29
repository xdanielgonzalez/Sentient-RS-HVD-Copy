//! `dds-probe` — a std host binary that drives the real `lib-dds` seam (WP5 #323), mirroring
//! `seplos-tcp-sim`'s split between a networked mode and an offline `dump-fixtures` mode.
//!
//! Three modes:
//! 1. **`subscribe --topic Heartbeat [--iface IFACE] [--timeout-secs N]`** — join the real DDS
//!    participant on `IFACE` (custom UDP/IPv6 transport, domain 0) and print received samples.
//!    Only `Heartbeat` is wired through `DustParticipant` today (WP6 adds the rest).
//! 2. **`publish --device-id ID [--iface IFACE] [--count N] [--interval-ms N]`** — publish N
//!    `Heartbeat` samples (real `lib_tms::cdr`-rendered XCDR2 bytes) for CycloneDDS interop.
//! 3. **`dump-fixtures`** — no networking: renders the identity-topic fixtures offline, the SAME
//!    canned values `deploy/dds-probe/dump_reference.py` uses, for a Rust<->Python cross-check.
//!
//! The HEAVY interop asserting (USER_DATA, late-joiner TRANSIENT_LOCAL replay, Reply KEEP_LAST
//! depth, CycloneDDS decode) lives in the containerized python probe
//! (`deploy/dds-probe/subscribe_assert.py`) — this bin is deliberately minimal.

mod impl_app_error;
mod impl_cli;
mod impl_core_fixtures;
mod impl_core_publish;
mod impl_core_render;
mod impl_core_subscribe;
mod prelude;
mod types;

use prelude::*;

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("dds-probe: error: {e}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), AppError> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cli = Cli::parse(&args)?;

    match cli.mode {
        Mode::DumpFixtures => {
            impl_core_fixtures::dump_fixtures();
            Ok(())
        }
        Mode::Subscribe {
            iface,
            timeout_secs,
        } => impl_core_subscribe::run_subscribe(iface, Duration::from_secs(timeout_secs)).await,
        Mode::Publish {
            iface,
            device_id,
            count,
            interval_ms,
        } => {
            impl_core_publish::run_publish(
                iface,
                device_id,
                count,
                Duration::from_millis(interval_ms),
            )
            .await
        }
    }
}

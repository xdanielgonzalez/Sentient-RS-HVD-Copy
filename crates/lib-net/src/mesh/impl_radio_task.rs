//! Serial-port open machinery for [`crate::mesh::run_radio`]. The receive/rate-limit/send/reconnect
//! loop lives on the free fn itself (`lib.rs`, since `run_radio` has no associated type to split
//! an `impl` block from); this file holds the narrow "how do we open the actual `tokio-serial`
//! port" concern — CLAUDE.md's "Embedded / Firmware Engineering Rules" keep-the-platform-call-site
//! dumb guidance: one-shot open, no retry/backoff/policy here, that belongs to the loop.

use super::prelude::*;
use tokio_serial::{SerialPortBuilderExt, SerialStream};

/// Backoff after a failed port open: starts here, doubles each consecutive failure, caps at
/// [`MAX_BACKOFF`] — a bounded reconnect, never a busy loop.
pub(crate) const INITIAL_BACKOFF: std::time::Duration = std::time::Duration::from_secs(1);
pub(crate) const MAX_BACKOFF: std::time::Duration = std::time::Duration::from_secs(30);

/// Open the Meshtastic radio's serial port per `cfg`. A thin, one-shot `tokio-serial` wrapper —
/// no loop, no retry, no policy (that's [`crate::mesh::run_radio`]'s job).
pub(crate) fn open_port(cfg: &MeshConfig) -> Result<SerialStream, RadioError> {
    tokio_serial::new(cfg.device_path.as_str(), cfg.baud)
        .open_native_async()
        .map_err(|e| RadioError::Io(e.to_string()))
}

#[cfg(test)]
// Test code may unwrap/expect/panic; the workspace lint gate denies these in production logic
// only.
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn opening_a_nonexistent_device_reports_a_typed_io_error_not_a_panic() {
        let cfg = MeshConfig {
            device_path: "/dev/definitely-not-a-real-mesh-radio-port".to_string(),
            baud: 115_200,
            channel: 0,
            max_payload: 200,
            send_interval_ms: 30_000,
        };
        match open_port(&cfg) {
            Err(RadioError::Io(_)) => {}
            other => panic!("expected RadioError::Io for a nonexistent device, got {other:?}"),
        }
    }

    #[test]
    fn backoff_bounds_are_sane() {
        assert!(INITIAL_BACKOFF < MAX_BACKOFF);
    }
}

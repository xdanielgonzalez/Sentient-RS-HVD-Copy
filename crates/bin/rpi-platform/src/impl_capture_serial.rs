use std::sync::atomic::Ordering;
use std::time::{SystemTime, UNIX_EPOCH};

use super::prelude::*;

/// Wall-clock milliseconds "now", for [`RawCaptureRecord::ts_ms`] — same saturate-to-0-past-epoch
/// convention as `impl_host_tap.rs`'s `wall_clock_ms` (duplicated rather than shared: each call
/// site is a two-line pure fn, not worth a shared helper module for).
fn wall_clock_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

impl SerialTransport for CaptureSerial {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, IoError> {
        self.inner.read(buf).await
    }

    /// Delegates to `inner`, then — when `handle` is `Some` — pairs the just-drained `pending_tx`
    /// (the request most recently staged by [`CaptureSerial::write_all`]) with `buf[..n]` (the
    /// response) into one [`RawCaptureRecord`], offered to the capture writer via a bounded
    /// `try_send`. Never applied as backpressure to the poll loop: a full or closed channel counts
    /// `handle.dropped` and warns once (not on every subsequent drop, to avoid a log storm on a
    /// stuck writer).
    ///
    /// Exchanges are strictly sequential per bus (one `write_all` fully completes before the
    /// `read_until` that answers it), so pairing the LAST staged write with the NEXT successful read
    /// is the correct exchange for every SINGLE-write/SINGLE-read driver call
    /// (`lib_io::driver::read_telemetry`/`read_fault`) — **except** the inverter driver's
    /// continuation-read retries (`lib_io::driver::exchange_inverter`, up to 2 extra `read_until`
    /// calls against the SAME `write_all`, recovering from the checksum-byte-equals-`\r` framing
    /// hazard): each continuation read still lands here with an already-drained (empty)
    /// `pending_tx`, so it is captured as its OWN record (empty request, the continuation bytes as
    /// response) rather than merged into the first. This is a known, benign fragmentation for that
    /// one driver path — every byte that crossed the wire is still captured, just split across more
    /// than one JSONL line for that one logical exchange. Deferred, not fixed here (issue #268 is
    /// the separate, unrelated `discard_input` desync).
    ///
    /// `valid` is TRANSPORT-level only: `true` whenever this arm runs at all (the read itself
    /// succeeded). This decorator sits BELOW `validate_frame`/parsing, so it cannot observe whether
    /// the frame it captured later parses cleanly — see [`RawCaptureRecord::valid`]'s doc comment
    /// for the field's fuller (parse-aware) intent, which this producer narrows to what a transport
    /// decorator can actually know.
    ///
    /// **Fast path**: when `handle` is `None` this is a pure passthrough — nothing here allocates
    /// or copies beyond the delegated read itself (the `pending_tx` push this pairs with is itself
    /// skipped in `write_all` whenever `handle` is `None` — see that method).
    async fn read_until(&mut self, delimiter: u8, buf: &mut [u8]) -> Result<usize, IoError> {
        let n = self.inner.read_until(delimiter, buf).await?;
        if let Some(handle) = &self.handle {
            let record = RawCaptureRecord {
                ts_ms: wall_clock_ms(),
                bus: handle.bus.clone(),
                request: std::mem::take(&mut self.pending_tx),
                response: buf[..n].to_vec(),
                valid: true,
            };
            if handle.tx.try_send(record).is_err() {
                let prev = handle.dropped.fetch_add(1, Ordering::Relaxed);
                if prev == 0 {
                    log::warn!(
                        "capture: queue full/closed on bus {}; further drops are counted \
                         silently (see CaptureHandle::dropped)",
                        handle.bus
                    );
                }
            }
        }
        Ok(n)
    }

    /// Delegates to `inner`; when `handle` is `Some`, stages `bytes` into `pending_tx` for the next
    /// successful `read_until` to pair as this exchange's request. Skipped entirely — no
    /// allocation, no copy — when `handle` is `None`, the fast-path guarantee [`CaptureSerial`]'s
    /// doc comment promises.
    async fn write_all(&mut self, bytes: &[u8]) -> Result<(), IoError> {
        self.inner.write_all(bytes).await?;
        if self.handle.is_some() {
            self.pending_tx.clear();
            self.pending_tx.extend_from_slice(bytes);
        }
        Ok(())
    }

    async fn set_direction(&mut self, dir: Direction) -> Result<(), IoError> {
        self.inner.set_direction(dir).await
    }

    /// Delegates to `inner` unchanged — issue #268's `discard_input` seam, not a capture concern.
    async fn discard_input(&mut self) -> Result<(), IoError> {
        self.inner.discard_input().await
    }
}

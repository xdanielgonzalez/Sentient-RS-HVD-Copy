use super::prelude::*;

// ---------------------------------------------------------------------------------------------
// Wire-rate pacing for reply writes. This wraps the TRANSPORT (how reply bytes are written to
// the peer) — never the frame content, which stays `SeplosSim`'s alone.
// ---------------------------------------------------------------------------------------------

/// Reply bytes are written this many at a time when pacing is active: a real UART sends one byte
/// every `10/baud` seconds, but sleeping per-byte would be 160 syscalls for a full telemetry
/// reply. Batching into small chunks (each followed by a proportional sleep) keeps bytes
/// trickling onto the wire — never one instant burst — while keeping syscall count reasonable.
/// At 19200 baud an 8-byte chunk is paced roughly every 4.2ms.
const PACE_CHUNK_BYTES: usize = 8;

/// Write `bytes` to `writer`, paced at the wire rate implied by `pace`.
///
/// `pace` of `None` (or `baud == 0`) reproduces today's behavior exactly: one `write_all` +
/// `flush`, no delay. Otherwise: sleep the simulated turnaround, then write `bytes` in
/// `PACE_CHUNK_BYTES`-sized chunks, sleeping `chunk_len * 10 / baud` seconds after each chunk (10
/// bits/byte: 1 start + 8 data + 1 stop, matching 8N1) so total wire time is `bytes.len() * 10 /
/// baud` seconds regardless of chunk size. `TCP_NODELAY` is left set by the caller so pacing is
/// honest — no Nagle coalescing hides the trickle.
pub(super) fn write_paced<W: Write>(
    writer: &mut W,
    bytes: &[u8],
    pace: Option<PaceConfig>,
) -> io::Result<()> {
    let Some(pace) = pace.filter(|p| p.baud > 0) else {
        writer.write_all(bytes)?;
        return writer.flush();
    };

    if pace.turnaround_ms > 0 {
        std::thread::sleep(Duration::from_millis(pace.turnaround_ms));
    }

    let byte_interval = Duration::from_secs_f64(10.0 / f64::from(pace.baud));
    for chunk in bytes.chunks(PACE_CHUNK_BYTES) {
        writer.write_all(chunk)?;
        writer.flush()?;
        std::thread::sleep(byte_interval * chunk.len() as u32);
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    /// A `Write` sink that records how much wall-clock time elapsed before each write, so tests
    /// can assert on trickle-vs-burst delivery without a real socket.
    struct TimedSink {
        start: Instant,
        writes: Vec<(Duration, usize)>,
    }

    impl Write for TimedSink {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.writes.push((self.start.elapsed(), buf.len()));
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn unpaced_write_is_a_single_instant_burst() {
        let mut sink = TimedSink {
            start: Instant::now(),
            writes: Vec::new(),
        };
        write_paced(&mut sink, &[1, 2, 3, 4, 5], None).expect("write_paced");
        assert_eq!(sink.writes.len(), 1, "unpaced write must not be chunked");
        assert_eq!(sink.writes[0].1, 5);
        assert!(
            sink.writes[0].0 < Duration::from_millis(5),
            "unpaced write should not sleep, took {:?}",
            sink.writes[0].0
        );
    }

    #[test]
    fn zero_baud_behaves_as_unpaced() {
        let mut sink = TimedSink {
            start: Instant::now(),
            writes: Vec::new(),
        };
        let pace = Some(PaceConfig {
            baud: 0,
            turnaround_ms: 999,
        });
        write_paced(&mut sink, &[1, 2, 3], pace).expect("write_paced");
        assert_eq!(sink.writes.len(), 1, "baud=0 must not pace or chunk");
        assert!(sink.writes[0].0 < Duration::from_millis(5));
    }

    #[test]
    fn paced_write_chunks_and_trickles() {
        let mut sink = TimedSink {
            start: Instant::now(),
            writes: Vec::new(),
        };
        let bytes = vec![0xAAu8; 20]; // 3 chunks at PACE_CHUNK_BYTES=8: 8, 8, 4
        let pace = Some(PaceConfig {
            baud: 19_200,
            turnaround_ms: 0,
        });
        write_paced(&mut sink, &bytes, pace).expect("write_paced");
        assert_eq!(
            sink.writes.len(),
            3,
            "20 bytes at chunk=8 should be 3 writes"
        );
        assert_eq!(sink.writes[0].1, 8);
        assert_eq!(sink.writes[1].1, 8);
        assert_eq!(sink.writes[2].1, 4);
        // First chunk write happens ~immediately (before its own sleep); the last chunk's write
        // must land measurably later than the first, proving the sleeps between chunks land.
        assert!(
            sink.writes[2].0 > sink.writes[0].0 + Duration::from_millis(3),
            "chunks should be spaced out in time: {:?}",
            sink.writes
        );
    }
}

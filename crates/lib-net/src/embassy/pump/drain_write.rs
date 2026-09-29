//! [`drain_and_write`] — the generic drain-and-write core. Free function — no type/impl split (see
//! the repo's code-organization rule): there is no struct here beyond the trivial
//! [`super::DrainWriteError`] marker (declared in its own `type_*.rs`), mirroring
//! `crate::embassy::adapter::pumps`'s bare-async-fn convention.

use crate::link::BrokerCore;
use embedded_io_async::Write;

use super::DrainWriteError;

/// Drain every packet [`BrokerCore::drain_outgoing`] has staged for `slot` and write each one fully
/// to `writer` before asking for the next.
///
/// Loop shape (this IS the partial-write backpressure/resumption handling):
/// ```text
/// loop {
///     let n = broker.drain_outgoing(slot, out, render);
///     if n == 0 { return Ok(()); }
///     writer.write_all(&out[..n]).await?;   // resumes across partial writes internally
/// }
/// ```
/// `drain_outgoing` returns at most one whole framed packet per call (CONNACK/SUBACK/PINGRESP/one
/// QoS-0 PUBLISH) and never fragments one across calls — so `out[..n]` is always a complete packet,
/// and this loop's own contract (never call `drain_outgoing` again until the previous `out[..n]`
/// is FULLY written) is what the frozen `BrokerCore` contract requires of any caller.
/// [`embedded_io_async::Write::write_all`] is used directly rather than a hand-rolled loop over
/// `.write()`: its default impl (see `embedded-io-async` 0.6.1's `lib.rs`) already loops
/// `write()` until the whole slice is written, returning `Err` on the first IO error and treating
/// `Ok(0)` as a contract violation (the trait's own docs require implementations to never return
/// `Ok(0)` for a non-empty buffer) — exactly the "loop-until-written across write-ready/backpressure
/// cycles" resumption this deliverable needs, with no extra logic to get wrong.
///
/// Any write error is reported as [`DrainWriteError`] — per the frozen `BrokerCore` contract, a
/// short/failed write means the peer is gone, and the caller's only correct response is always the
/// same regardless of the underlying IO error's specifics: `client_closed(slot)` then close/re-listen
/// (deliverable 2 does exactly this). `out` is left in whatever state the failed write left it in;
/// the caller is expected to discard the connection, not retry `out`.
pub async fn drain_and_write<W, R, const MAX_CLIENTS: usize>(
    broker: &mut BrokerCore<MAX_CLIENTS>,
    slot: usize,
    writer: &mut W,
    out: &mut [u8],
    render: &mut R,
) -> Result<(), DrainWriteError>
where
    W: Write,
    R: FnMut(&str, &mut [u8]) -> Option<usize>,
{
    loop {
        let n = broker.drain_outgoing(slot, out, render);
        if n == 0 {
            return Ok(());
        }
        writer
            .write_all(&out[..n])
            .await
            .map_err(|_| DrainWriteError)?;
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use embedded_io_async::ErrorType;

    #[derive(Debug)]
    struct MockWriteError;
    // embedded-io 0.7 added a `core::error::Error` supertrait on `embedded_io_async::Error`
    // (was Debug-only in 0.6), so the mock error must also carry Display + core::error::Error.
    impl core::fmt::Display for MockWriteError {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.write_str("mock write error")
        }
    }
    impl core::error::Error for MockWriteError {}
    impl embedded_io_async::Error for MockWriteError {
        fn kind(&self) -> embedded_io_async::ErrorKind {
            embedded_io_async::ErrorKind::Other
        }
    }

    /// A mock [`embedded_io_async::Write`] that accepts only a FEW bytes per `write()` call,
    /// recording everything written into a `Vec<u8>` — proves [`drain_and_write`]'s reliance on
    /// `write_all`'s resumption is correct even under a tightly constrained transport window.
    struct ChunkedMockWriter {
        chunk: usize,
        written: std::vec::Vec<u8>,
    }

    impl ChunkedMockWriter {
        fn new(chunk: usize) -> Self {
            ChunkedMockWriter {
                chunk,
                written: std::vec::Vec::new(),
            }
        }
    }

    impl ErrorType for ChunkedMockWriter {
        type Error = MockWriteError;
    }

    impl embedded_io_async::Write for ChunkedMockWriter {
        async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
            if buf.is_empty() {
                return Ok(0);
            }
            let n = buf.len().min(self.chunk);
            self.written.extend_from_slice(&buf[..n]);
            Ok(n)
        }
        // `flush` is a required method in embedded-io-async 0.7 (had a default in 0.6); this mock
        // buffers into a Vec, so there is nothing to flush.
        async fn flush(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }
    }

    /// A mock that fails outright on the first `write()` call — proves a write error surfaces as
    /// [`DrainWriteError`].
    struct FailingWriter;
    impl ErrorType for FailingWriter {
        type Error = MockWriteError;
    }
    impl embedded_io_async::Write for FailingWriter {
        async fn write(&mut self, _buf: &[u8]) -> Result<usize, Self::Error> {
            Err(MockWriteError)
        }
        async fn flush(&mut self) -> Result<(), Self::Error> {
            Err(MockWriteError)
        }
    }

    #[tokio::test]
    async fn drains_and_writes_a_staged_publish_over_a_constrained_window_byte_exact() {
        const PAYLOAD_LEN: usize = 1600;
        let payload: std::vec::Vec<u8> = (0..PAYLOAD_LEN).map(|i| (i % 251) as u8).collect();

        let mut broker: BrokerCore<4> = BrokerCore::new();
        broker.client_connected(0);
        // Drive a real CONNECT + SUBSCRIBE through the public API (this test exercises
        // drain_and_write's write-side behavior, not BrokerCore's own protocol handling — that's
        // covered by lib-link's own frozen tests — but publish_topic only fans out to clients that
        // are actually subscribed, so a real SUBSCRIBE is needed to get a pending-publish entry
        // queued at all).
        let mut connect_buf = [0u8; 128];
        let n = crate::link::mqtt_client::encode_connect(
            &crate::link::mqtt_client::ConnectOptions {
                client_id: "dev01",
                keep_alive_secs: 60,
                clean_session: true,
                username: None,
                password: None,
            },
            &mut connect_buf,
        )
        .unwrap();
        broker.feed_incoming(0, &connect_buf[..n]).unwrap();

        // Hand-rolled SUBSCRIBE (mirrors crates/lib-link/src/broker_transport/tests.rs's
        // encode_subscribe helper — reimplemented here since mqtt_wire is private to lib-link).
        fn encode_subscribe(packet_id: u16, filter: &str) -> std::vec::Vec<u8> {
            let mut buf = std::vec::Vec::new();
            let remaining = 2 + 2 + filter.len() + 1;
            buf.push(0x82);
            let mut rl = remaining;
            loop {
                let mut byte = (rl % 128) as u8;
                rl /= 128;
                if rl > 0 {
                    byte |= 0x80;
                }
                buf.push(byte);
                if rl == 0 {
                    break;
                }
            }
            buf.extend_from_slice(&(packet_id).to_be_bytes());
            buf.extend_from_slice(&(filter.len() as u16).to_be_bytes());
            buf.extend_from_slice(filter.as_bytes());
            buf.push(0x00);
            buf
        }
        broker
            .feed_incoming(0, &encode_subscribe(1, "t/big"))
            .unwrap();
        broker.publish_topic("t/big", false);

        let payload_clone = payload.clone();
        let mut render = move |topic: &str, out: &mut [u8]| -> Option<usize> {
            if topic == "t/big" {
                out.get_mut(..PAYLOAD_LEN)?.copy_from_slice(&payload_clone);
                Some(PAYLOAD_LEN)
            } else {
                None
            }
        };

        let mut writer = ChunkedMockWriter::new(5); // 5 bytes per write() call
        let mut out = vec![0u8; crate::link::BrokerCore::<4>::min_out_len(PAYLOAD_LEN)];

        drain_and_write(&mut broker, 0, &mut writer, &mut out, &mut render)
            .await
            .unwrap();

        // CONNACK(4) + SUBACK(5) are drained/written first, then the PUBLISH; the payload is the
        // exact tail of everything written.
        assert!(writer.written.len() > PAYLOAD_LEN);
        assert_eq!(
            &writer.written[writer.written.len() - PAYLOAD_LEN..],
            &payload[..]
        );
    }

    #[tokio::test]
    async fn write_error_surfaces_as_drain_write_error() {
        let mut broker: BrokerCore<4> = BrokerCore::new();
        broker.client_connected(0);
        let mut connect_buf = [0u8; 128];
        let n = crate::link::mqtt_client::encode_connect(
            &crate::link::mqtt_client::ConnectOptions {
                client_id: "dev01",
                keep_alive_secs: 60,
                clean_session: true,
                username: None,
                password: None,
            },
            &mut connect_buf,
        )
        .unwrap();
        broker.feed_incoming(0, &connect_buf[..n]).unwrap();

        let mut writer = FailingWriter;
        let mut out = [0u8; 64];
        let mut render = |_: &str, _: &mut [u8]| -> Option<usize> { None };

        let result = drain_and_write(&mut broker, 0, &mut writer, &mut out, &mut render).await;
        assert_eq!(result, Err(DrainWriteError));
    }
}

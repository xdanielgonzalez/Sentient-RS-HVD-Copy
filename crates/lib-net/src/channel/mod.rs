//! `lib-channel` — the inter-stage delineation layer.
//!
//! The pipeline is a chain of independently-hardenable **stages** connected by typed, bounded
//! **channels**. This crate is the connective tissue: the channel + stage *contracts* and the
//! *boundary messages* that cross them. It has **zero runtime dependencies** — the concrete channel
//! (embassy channel on the MCU, tokio-mpsc on the host, or a Unix-socket/shared-memory transport
//! for isolated processes) is chosen per platform and implements these traits.
//!
//! ```text
//! [UART link] --UartChunk--> [parse] -> reading -> [map] --MqttUnit--> [MQTT]
//!    --CryptoUnit--> [crypto/TLS] --WireFrame--> [net link] --> wire
//! ```
//!
//! The `[crypto/TLS]` slot is now real for its seal/open leg: [`crypto_stage::CryptoStage`]
//! (feature `crypto`) is the [`stage::Stage`] that takes a [`message::CryptoUnit`] in and hands a
//! sealed/opened one back out, riding `lib-security`'s `SessionCipher` seam — see that module's
//! doc for the AAD/capacity/error-mapping decisions. The TLS record layer
//! (`lib_net::link::tls::TlsStage`) remains a separate, not-yet-composed leg of the same slot: it
//! drives a raw byte stream (handshake + record framing), not this channel's `CryptoUnit`
//! boundary, so the two are not (yet) wired together.
//!
//! Four boundary channels — [`message::UartChunk`] (serial), [`message::MqttUnit`] (dissemination),
//! [`message::CryptoUnit`] (the crypto handoff — data + an opaque key context, never key bytes),
//! and [`message::WireFrame`] (raw L2). Why this shape:
//! * **Modular / process-independent** — a stage owns no global state and reaches only its input,
//!   so it runs as its own task or process, wired only by channels.
//! * **Hardenable in isolation** — feed a stage an input message, assert its output; each boundary
//!   is a fault-injection + byte-verification point.
//! * **Strict bytes on the wire** — every boundary message is bounded (heapless) and, under
//!   `feature = "packed"`, encodes to compact schema-strict bytes (postcard) — so an IPC transport
//!   carries packed bytes (not JSON) and the exact bytes are capture-vector-testable, the same
//!   discipline the Seplos frame vectors use.

// The `channel` submodule (Sender/Receiver contract) keeps its name from the former `lib-channel`
// crate's own internal structure (re-exported flat below, so the public path stays
// `lib_net::channel::Sender`). Inception is the faithful fold of that structure, not a smell.
#[allow(clippy::module_inception)]
pub mod channel;
pub mod message;
pub mod stage;

#[cfg(feature = "packed")]
pub mod codec;

/// The real seal/open leg of the `[crypto/TLS]` pipeline slot, over `lib-security`'s
/// `SessionCipher` seam. Feature `crypto`.
#[cfg(feature = "crypto")]
pub mod crypto_stage;

pub use channel::{Closed, Receiver, Sender, TryRecvError, TrySendError};
pub use message::{CryptoOp, CryptoUnit, FlowDir, MqttUnit, UartChunk, WireFrame};
pub use stage::{drive, Stage};

#[cfg(feature = "crypto")]
pub use crypto_stage::{CryptoStage, CryptoStageError};

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use heapless::Deque;

    /// A minimal in-memory bounded channel (single owner) — exercises the `try_*` contract without
    /// an executor. Real async channels (tokio-mpsc / embassy) implement the full trait per platform.
    ///
    /// `lib-channel` itself defines only the [`Sender`]/[`Receiver`] *traits* and the `Closed`
    /// variants — no concrete channel implementation lives in this crate (the tokio-mpsc host impl
    /// is `lib-link`, out of scope for this test module). `close()` here is a test-only stand-in so
    /// the `TrySendError::Closed` / `TryRecvError::Closed` variants have *something* to exercise;
    /// it does not claim to reproduce a real implementation's shutdown ordering.
    struct RingChannel<T, const N: usize> {
        q: Deque<T, N>,
        closed: bool,
    }
    impl<T, const N: usize> RingChannel<T, N> {
        fn new() -> Self {
            RingChannel {
                q: Deque::new(),
                closed: false,
            }
        }
        fn close(&mut self) {
            self.closed = true;
        }
        fn push(&mut self, item: T) -> Result<(), TrySendError<T>> {
            if self.closed {
                return Err(TrySendError::Closed(item));
            }
            self.q.push_back(item).map_err(TrySendError::Full)
        }
        fn pop(&mut self) -> Result<T, TryRecvError> {
            match self.q.pop_front() {
                Some(item) => Ok(item),
                None if self.closed => Err(TryRecvError::Closed),
                None => Err(TryRecvError::Empty),
            }
        }
    }

    fn uart(bytes: &[u8]) -> UartChunk {
        UartChunk {
            dir: FlowDir::Ingress,
            data: heapless::Vec::from_slice(bytes).unwrap(),
        }
    }

    #[test]
    fn bounded_channel_backpressures_not_drops() {
        let mut ch: RingChannel<UartChunk, 2> = RingChannel::new();
        assert!(ch.push(uart(b"~2000")).is_ok());
        assert!(ch.push(uart(b"46")).is_ok());
        // Full → returns the item (back-pressure), never a silent drop.
        match ch.push(uart(b"F001")) {
            Err(TrySendError::Full(item)) => assert_eq!(item, uart(b"F001")),
            other => panic!("expected Full, got {other:?}"),
        }
        // Drain in order.
        assert_eq!(ch.pop().unwrap(), uart(b"~2000"));
        assert_eq!(ch.pop().unwrap(), uart(b"46"));
        assert_eq!(ch.pop(), Err(TryRecvError::Empty));
    }

    #[test]
    fn closed_channel_send_returns_item() {
        let mut ch: RingChannel<UartChunk, 2> = RingChannel::new();
        ch.close();
        // A closed channel refuses new sends and hands the item back — never a silent drop, and
        // distinct from `Full` (capacity is unrelated to closedness).
        match ch.push(uart(b"AA")) {
            Err(TrySendError::Closed(item)) => assert_eq!(item, uart(b"AA")),
            other => panic!("expected Closed, got {other:?}"),
        }
    }

    #[test]
    fn closed_channel_recv_drains_then_reports_closed() {
        let mut ch: RingChannel<UartChunk, 2> = RingChannel::new();
        assert!(ch.push(uart(b"one")).is_ok());
        assert!(ch.push(uart(b"two")).is_ok());
        ch.close();
        // Closing does not discard what's already queued — drain in order first.
        assert_eq!(ch.pop().unwrap(), uart(b"one"));
        assert_eq!(ch.pop().unwrap(), uart(b"two"));
        // Once drained AND closed, recv reports `Closed`, not `Empty`.
        assert_eq!(ch.pop(), Err(TryRecvError::Closed));
    }

    #[test]
    fn capacity_boundary_n1_full_pop_push_cycle() {
        let mut ch: RingChannel<UartChunk, 1> = RingChannel::new();
        assert!(ch.push(uart(b"a")).is_ok());
        // Full at N=1 after a single push.
        match ch.push(uart(b"b")) {
            Err(TrySendError::Full(item)) => assert_eq!(item, uart(b"b")),
            other => panic!("expected Full, got {other:?}"),
        }
        // Popping frees exactly one slot.
        assert_eq!(ch.pop().unwrap(), uart(b"a"));
        assert!(ch.push(uart(b"c")).is_ok());
        // The single slot is occupied again — a second push still fails.
        match ch.push(uart(b"d")) {
            Err(TrySendError::Full(item)) => assert_eq!(item, uart(b"d")),
            other => panic!("expected Full, got {other:?}"),
        }
        assert_eq!(ch.pop().unwrap(), uart(b"c"));
        assert_eq!(ch.pop(), Err(TryRecvError::Empty));
    }

    #[test]
    fn wire_frame_new_at_exactly_mtu_is_some() {
        let bytes = [0xABu8; message::WIRE_MTU];
        let frame = WireFrame::new(FlowDir::Egress, &bytes).expect("MTU-sized frame must fit");
        assert_eq!(frame.dir, FlowDir::Egress);
        assert_eq!(frame.data.len(), message::WIRE_MTU);
        assert_eq!(frame.data.as_slice(), &bytes[..]);
    }

    #[test]
    fn wire_frame_new_over_mtu_by_one_is_none() {
        let bytes = [0xABu8; message::WIRE_MTU + 1];
        assert!(WireFrame::new(FlowDir::Egress, &bytes).is_none());
    }

    #[test]
    fn wire_frame_empty_preserves_dir_and_is_zero_len() {
        let ingress = WireFrame::empty(FlowDir::Ingress);
        assert_eq!(ingress.dir, FlowDir::Ingress);
        assert_eq!(ingress.data.len(), 0);

        let egress = WireFrame::empty(FlowDir::Egress);
        assert_eq!(egress.dir, FlowDir::Egress);
        assert_eq!(egress.data.len(), 0);
    }

    #[cfg(feature = "packed")]
    #[test]
    fn boundary_messages_round_trip_packed() {
        // Every boundary message encodes to compact bytes and decodes back byte-identically.
        let mut buf = [0u8; 512];

        let u = uart(b"~20004642F0010FD67\r");
        let n = codec::encode(&u, &mut buf).unwrap();
        assert_eq!(codec::decode::<UartChunk>(&buf[..n]).unwrap(), u);

        let mut topic = heapless::String::new();
        topic
            .push_str("tms/METEOR-001/dc_measurement_update")
            .unwrap();
        let m = MqttUnit {
            topic,
            payload: heapless::Vec::from_slice(br#"{"amperage":1.44}"#).unwrap(),
            qos: 1,
        };
        let n = codec::encode(&m, &mut buf).unwrap();
        assert_eq!(codec::decode::<MqttUnit>(&buf[..n]).unwrap(), m);

        let c = CryptoUnit {
            op: CryptoOp::Seal,
            context: 7,
            data: heapless::Vec::from_slice(b"plaintext").unwrap(),
        };
        let n = codec::encode(&c, &mut buf).unwrap();
        assert_eq!(codec::decode::<CryptoUnit>(&buf[..n]).unwrap(), c);

        // CryptoOp::Open must round-trip distinctly from Seal (an enum-variant coverage gap: the
        // Seal case above alone wouldn't catch a mixed-up discriminant).
        let c_open = CryptoUnit {
            op: CryptoOp::Open,
            context: 9,
            data: heapless::Vec::from_slice(b"ciphertext").unwrap(),
        };
        let n = codec::encode(&c_open, &mut buf).unwrap();
        let decoded = codec::decode::<CryptoUnit>(&buf[..n]).unwrap();
        assert_eq!(decoded, c_open);
        assert_eq!(decoded.op, CryptoOp::Open);

        // WireFrame was missing from this round-trip sweep entirely.
        let w = WireFrame::new(FlowDir::Ingress, b"\x00\x11\x22\x33 frame bytes").unwrap();
        let n = codec::encode(&w, &mut buf).unwrap();
        assert_eq!(codec::decode::<WireFrame>(&buf[..n]).unwrap(), w);

        let w_empty = WireFrame::empty(FlowDir::Egress);
        let n = codec::encode(&w_empty, &mut buf).unwrap();
        assert_eq!(codec::decode::<WireFrame>(&buf[..n]).unwrap(), w_empty);
    }

    #[cfg(feature = "packed")]
    #[test]
    fn decode_rejects_every_strict_prefix_of_a_valid_encoding() {
        // The prior version of this test (`decode_is_fail_closed`) asserted
        // `is_err() || true`, which is a tautology — it can never fail no matter what `decode`
        // does. Replace it with a real truncation sweep: encode a valid unit, then assert that
        // *every* strict prefix (0..n bytes, i.e. excluding the full valid encoding) fails to
        // decode — and, since this runs to completion under `#[test]`, that it never panics.
        let u = uart(b"~20004642F0010FD67\r");
        let mut buf = [0u8; 64];
        let n = codec::encode(&u, &mut buf).unwrap();
        assert!(n > 1, "sanity: encoding must be more than one byte");

        // Sanity: the full encoding does decode.
        assert_eq!(codec::decode::<UartChunk>(&buf[..n]).unwrap(), u);

        for len in 0..n {
            assert!(
                codec::decode::<UartChunk>(&buf[..len]).is_err(),
                "expected decode to fail closed on a {len}-byte strict prefix of a {n}-byte encoding"
            );
        }

        // Also cover the plainly-malformed cases from the original test.
        assert!(codec::decode::<UartChunk>(&[0xFFu8; 4]).is_err());
        assert!(codec::decode::<MqttUnit>(&[]).is_err());
    }

    #[cfg(feature = "packed")]
    #[test]
    fn encode_fails_closed_on_small_buffer() {
        let mut tiny = [0u8; 2];
        let big = MqttUnit {
            topic: heapless::String::new(),
            payload: heapless::Vec::from_slice(&[0u8; 64]).unwrap(),
            qos: 0,
        };
        assert_eq!(
            codec::encode(&big, &mut tiny),
            Err(codec::CodecError::BufferTooSmall)
        );
    }

    #[cfg(feature = "packed")]
    #[test]
    fn encode_into_exactly_sized_buffer_succeeds_n_minus_one_fails() {
        let u = uart(b"~20004642F0010FD67\r");

        // First discover the exact encoded length with a generously sized buffer.
        let mut probe = [0u8; 64];
        let n = codec::encode(&u, &mut probe).unwrap();

        // A buffer of exactly the encoded length succeeds and uses the whole thing.
        let mut exact = vec![0u8; n];
        let used = codec::encode(&u, &mut exact).unwrap();
        assert_eq!(used, n);
        assert_eq!(codec::decode::<UartChunk>(&exact[..used]).unwrap(), u);

        // One byte short fails closed with the buffer-too-small variant, not a panic or a
        // silently truncated write.
        let mut short = vec![0u8; n - 1];
        assert_eq!(
            codec::encode(&u, &mut short),
            Err(codec::CodecError::BufferTooSmall)
        );
    }

    #[cfg(feature = "packed")]
    #[test]
    fn codec_error_maps_every_variant_to_try_recv_error() {
        // Both `CodecError` variants currently map to `TryRecvError::Empty` — a malformed or
        // too-small framed message reads as "nothing valid to receive" rather than exposing the
        // codec's internal distinction across the channel boundary. Pin that mapping explicitly
        // per variant so a future change to one arm doesn't silently change behavior unnoticed.
        let from_malformed: TryRecvError = codec::CodecError::Malformed.into();
        assert_eq!(from_malformed, TryRecvError::Empty);

        let from_too_small: TryRecvError = codec::CodecError::BufferTooSmall.into();
        assert_eq!(from_too_small, TryRecvError::Empty);
    }
}

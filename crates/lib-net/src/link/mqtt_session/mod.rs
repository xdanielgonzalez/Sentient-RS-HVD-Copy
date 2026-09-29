//! The owned MQTT 3.1.1 client *session*: the connection-flow state machine that sequences the
//! wire encoders/parser in [`crate::link::mqtt_client`] into a working `CONNECT -> CONNACK -> PUBLISH`
//! flow with a bounded keep-alive `PINGREQ`.
//!
//! [`crate::link::mqtt_client`] owns the byte format of individual control packets; this module owns the
//! *ordering* between them (send CONNECT first, don't PUBLISH until CONNACK is accepted, emit a
//! PINGREQ when the link has been idle for a keep-alive interval). It is transport-agnostic and
//! caller-driven, mirroring [`crate::link::tls::TlsStage`]'s `feed_incoming` / `drain_outgoing` seam: the
//! caller polls it with the current clock and any freshly-received plaintext, and the session hands
//! back the plaintext bytes to write to the transport. That keeps the same one module usable over
//! either a raw [`crate::link::NetStack`] TCP socket (plaintext broker) or a `TlsStage` (MQTT-over-TLS) —
//! the session never learns which.
//!
//! `no_std`/no-alloc: caller-owned time (`now_ms: i64`, the crate-wide poll clock — see
//! [`crate::link::NetStack::poll`]) and fixed-capacity `heapless` staging buffers, so it builds for the
//! MCU target alongside the rest of the packet/session layer. Keep-alive is therefore bounded by
//! construction — a PINGREQ is only staged on a poll whose `now_ms` has advanced past the interval,
//! never on a wall-clock timer or an unbounded sleep.

mod prelude;
mod types;

mod impl_mqtt_session;
mod impl_session_error;

pub use types::*;

#[cfg(test)]
use prelude::*;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn opts() -> ConnectOptions<'static> {
        ConnectOptions {
            client_id: "dev01",
            keep_alive_secs: 60,
            clean_session: true,
            username: None,
            password: None,
        }
    }

    const CONNECT_VECTOR: [u8; 19] = [
        0x10, 0x11, 0x00, 0x04, 0x4D, 0x51, 0x54, 0x54, 0x04, 0x02, 0x00, 0x3C, 0x00, 0x05, 0x64,
        0x65, 0x76, 0x30, 0x31,
    ];
    const CONNACK_ACCEPTED: [u8; 4] = [0x20, 0x02, 0x00, 0x00];
    const PUBLISH_QOS0_VECTOR: [u8; 9] = [0x30, 0x07, 0x00, 0x03, 0x74, 0x2F, 0x31, 0x68, 0x69];

    #[test]
    fn first_poll_emits_connect_then_awaits_connack() {
        let mut s = MqttSession::new(opts());
        let mut out = [0u8; 64];
        let n = s.poll(0, &[], &mut out).unwrap();
        assert_eq!(&out[..n], &CONNECT_VECTOR, "first poll must emit CONNECT");
        assert!(!s.is_connected(), "not connected until CONNACK");
    }

    #[test]
    fn connack_accepted_transitions_to_connected() {
        let mut s = MqttSession::new(opts());
        let mut out = [0u8; 64];
        let _ = s.poll(0, &[], &mut out).unwrap();
        let n = s.poll(10, &CONNACK_ACCEPTED, &mut out).unwrap();
        assert_eq!(n, 0, "nothing to send on the poll that consumes CONNACK");
        assert!(s.is_connected());
    }

    #[test]
    fn publish_before_connack_is_rejected() {
        let mut s = MqttSession::new(opts());
        assert_eq!(s.publish("t/1", b"hi"), Err(SessionError::NotConnected));
    }

    #[test]
    fn publish_after_connack_emits_wire_exact_packet() {
        let mut s = MqttSession::new(opts());
        let mut out = [0u8; 64];
        let _ = s.poll(0, &[], &mut out).unwrap();
        let _ = s.poll(10, &CONNACK_ACCEPTED, &mut out).unwrap();
        s.publish("t/1", b"hi").unwrap();
        let n = s.poll(20, &[], &mut out).unwrap();
        assert_eq!(
            &out[..n],
            &PUBLISH_QOS0_VECTOR,
            "PUBLISH must be byte-exact once connected"
        );
    }

    #[test]
    fn idle_link_emits_pingreq_after_keep_alive_interval() {
        let mut s = MqttSession::new(opts()); // keep-alive 60s = 60_000 ms
        let mut out = [0u8; 64];
        let _ = s.poll(0, &[], &mut out).unwrap();
        let _ = s.poll(10, &CONNACK_ACCEPTED, &mut out).unwrap();
        // Well within the interval — no PINGREQ.
        assert_eq!(s.poll(1000, &[], &mut out).unwrap(), 0);
        // Past a full keep-alive interval of idleness — one PINGREQ.
        let n = s.poll(70_000, &[], &mut out).unwrap();
        assert_eq!(&out[..n], &[0xC0, 0x00], "an idle link should PINGREQ");
    }

    #[test]
    fn a_pingresp_is_consumed_and_does_not_block_the_session() {
        let mut s = MqttSession::new(opts());
        let mut out = [0u8; 64];
        let _ = s.poll(0, &[], &mut out).unwrap();
        let _ = s.poll(10, &CONNACK_ACCEPTED, &mut out).unwrap();
        // A PINGRESP arrives; it is silently consumed and publishing still works.
        let _ = s.poll(20, &[0xD0, 0x00], &mut out).unwrap();
        s.publish("t/1", b"hi").unwrap();
        let n = s.poll(30, &[], &mut out).unwrap();
        assert_eq!(&out[..n], &PUBLISH_QOS0_VECTOR);
    }

    #[test]
    fn refused_connack_surfaces_the_return_code() {
        let mut s = MqttSession::new(opts());
        let mut out = [0u8; 64];
        let _ = s.poll(0, &[], &mut out).unwrap();
        // Return code 5 = "not authorized" (MQTT 3.1.1 section 3.2.2.3).
        let refused = [0x20, 0x02, 0x00, 0x05];
        assert_eq!(
            s.poll(10, &refused, &mut out),
            Err(SessionError::Refused(5))
        );
        assert!(!s.is_connected());
    }
}

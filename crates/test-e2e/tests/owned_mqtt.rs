//! An owned MQTT 3.1.1 client (`lib_net::link::mqtt_client`) — wire-exact CONNECT / PUBLISH /
//! PINGREQ encoders and a CONNACK parser, per the OASIS MQTT v3.1.1 spec, plus one loopback PUBLISH
//! through the TLS stage to prove the pieces compose.
//!
//! There is no owned MQTT client elsewhere in this workspace — `lib-mqtt` is the embedded
//! rumqttd *broker* crate (server-side), and the design replaces the client-side rumqttd/mosquitto
//! path with this owned encoder. These vectors are the wire contract it must produce byte-exact.

// Test code opts out of the workspace's `unwrap_used`/`expect_used`/`panic` lint (production posture
// only) — same convention as the existing unit tests in this crate.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use lib_net::link::mqtt_client::{
    encode_connect, encode_pingreq, encode_publish, parse_connack, ConnectOptions,
};

/// CONNECT for client id "dev01", keep-alive 60s, clean session, no auth/will — MQTT 3.1.1 §3.1.
/// `10 11` (fixed header: type=CONNECT, remaining length=17) `00 04 4D 51 54 54` (Protocol Name
/// "MQTT") `04` (Protocol Level 4) `02` (Connect Flags: CleanSession only) `00 3C` (Keep Alive = 60)
/// `00 05 64 65 76 30 31` (Client Identifier "dev01").
const CONNECT_VECTOR: [u8; 19] = [
    0x10, 0x11, 0x00, 0x04, 0x4D, 0x51, 0x54, 0x54, 0x04, 0x02, 0x00, 0x3C, 0x00, 0x05, 0x64, 0x65,
    0x76, 0x30, 0x31,
];

/// PUBLISH (QoS 0, no DUP/RETAIN) of payload `hi` to topic `t/1` — MQTT 3.1.1 §3.3. `30 07` (fixed
/// header: type=PUBLISH, flags=0, remaining length=7) `00 03 74 2F 31` (Topic Name "t/1", no packet
/// id at QoS 0) `68 69` (payload "hi").
const PUBLISH_QOS0_VECTOR: [u8; 9] = [0x30, 0x07, 0x00, 0x03, 0x74, 0x2F, 0x31, 0x68, 0x69];

/// PINGREQ — MQTT 3.1.1 §3.13: always exactly `C0 00` (type=PINGREQ, remaining length=0).
const PINGREQ_VECTOR: [u8; 2] = [0xC0, 0x00];

/// CONNACK accepted, no session present — MQTT 3.1.1 §3.2: `20 02` (fixed header) `00` (Connect
/// Acknowledge Flags: session present = 0) `00` (Return Code: accepted).
const CONNACK_ACCEPTED_VECTOR: [u8; 4] = [0x20, 0x02, 0x00, 0x00];

/// CONNACK accepted, session present — same as above with the session-present bit set.
const CONNACK_SESSION_PRESENT_VECTOR: [u8; 4] = [0x20, 0x02, 0x01, 0x00];

#[test]
fn encode_connect_matches_mqtt311_wire_vector() {
    let opts = ConnectOptions {
        client_id: "dev01",
        keep_alive_secs: 60,
        clean_session: true,
        username: None,
        password: None,
    };
    let mut out = [0u8; 32];
    let n = encode_connect(&opts, &mut out).expect("encode_connect should succeed");
    assert_eq!(
        &out[..n],
        &CONNECT_VECTOR,
        "CONNECT must be byte-exact to the MQTT 3.1.1 wire format"
    );
}

#[test]
fn encode_publish_qos0_matches_mqtt311_wire_vector() {
    let mut out = [0u8; 32];
    let n = encode_publish("t/1", b"hi", 0, None, &mut out).expect("encode_publish should succeed");
    assert_eq!(
        &out[..n],
        &PUBLISH_QOS0_VECTOR,
        "PUBLISH (QoS 0) must be byte-exact to the MQTT 3.1.1 wire format"
    );
}

#[test]
fn encode_pingreq_is_the_fixed_two_byte_packet() {
    let mut out = [0u8; 8];
    let n = encode_pingreq(&mut out).expect("encode_pingreq should succeed");
    assert_eq!(
        &out[..n],
        &PINGREQ_VECTOR,
        "PINGREQ is always exactly 0xC0 0x00 per MQTT 3.1.1"
    );
}

#[test]
fn parse_connack_extracts_return_code_when_no_session_present() {
    let ack = parse_connack(&CONNACK_ACCEPTED_VECTOR).expect("parse_connack should succeed");
    assert!(!ack.session_present);
    assert_eq!(ack.return_code, 0, "return code 0 = Connection Accepted");
}

#[test]
fn parse_connack_extracts_session_present_flag() {
    let ack = parse_connack(&CONNACK_SESSION_PRESENT_VECTOR).expect("parse_connack should succeed");
    assert!(ack.session_present);
    assert_eq!(ack.return_code, 0);
}

/// The loopback composition test needs the TLS stage; `lib-link`'s `tls` feature is always on
/// for this crate's dev-dependency (default features), same as `tls_stage.rs`.
mod loopback {
    use super::*;
    use lib_net::link::tls::{TlsStage, TLS_CHUNK};
    use lib_net::link::tls_identity::SoftwareTlsIdentity;
    use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
    use smoltcp::iface::SocketStorage;
    use smoltcp::socket::tcp::State;
    use smoltcp::wire::{IpAddress, IpEndpoint, Ipv4Address, Ipv4Cidr};
    use test_kit::LoopbackBridge;

    /// A minimal, test-only PUBLISH reader mirroring MQTT 3.1.1 §3.3 fixed-header + variable-header
    /// layout at QoS 0 — deliberately *not* a production `parse_publish` (out of scope for this
    /// stub's surface), just enough to let the "broker" side of the test assert what it received.
    fn topic_and_payload_of_qos0_publish(bytes: &[u8]) -> (&str, &[u8]) {
        assert_eq!(bytes[0] & 0xF0, 0x30, "expected a PUBLISH packet");
        let topic_len = u16::from_be_bytes([bytes[2], bytes[3]]) as usize;
        let topic = core::str::from_utf8(&bytes[4..4 + topic_len]).expect("topic is UTF-8");
        let payload = &bytes[4 + topic_len..];
        (topic, payload)
    }

    #[test]
    fn publish_round_trips_through_tls_loopback_to_broker_topic_and_payload() {
        const CLIENT_MAC: [u8; 6] = [0x02, 0x00, 0x00, 0x00, 0x00, 0x05];
        const SERVER_MAC: [u8; 6] = [0x02, 0x00, 0x00, 0x00, 0x00, 0x06];
        const CLIENT_IP: Ipv4Address = Ipv4Address::new(10, 0, 2, 1);
        const SERVER_IP: Ipv4Address = Ipv4Address::new(10, 0, 2, 2);
        const SERVER_PORT: u16 = 8883; // MQTT-over-TLS conventional port
        const CLIENT_PORT: u16 = 55002;
        const TOPIC: &str = "sentient/dev01/tms";
        const PAYLOAD: &[u8] = b"{\"v\":1}";
        const MAX_POLLS: usize = 500;

        let (mut device_client, mut device_server) = LoopbackBridge::new(1514).split();

        let mut client_sockets: [SocketStorage; 1] = Default::default();
        let mut server_sockets: [SocketStorage; 1] = Default::default();
        let mut client_stack = lib_net::link::NetStack::new(
            &mut device_client,
            CLIENT_MAC,
            Ipv4Cidr::new(CLIENT_IP, 24),
            None,
            &mut client_sockets,
        );
        let mut server_stack = lib_net::link::NetStack::new(
            &mut device_server,
            SERVER_MAC,
            Ipv4Cidr::new(SERVER_IP, 24),
            None,
            &mut server_sockets,
        );

        let mut client_rx = [0u8; 4096];
        let mut client_tx = [0u8; 4096];
        let mut server_rx = [0u8; 4096];
        let mut server_tx = [0u8; 4096];
        let client_handle = client_stack.add_tcp(&mut client_rx, &mut client_tx);
        let server_handle = server_stack.add_tcp(&mut server_rx, &mut server_tx);

        server_stack
            .tcp(server_handle)
            .listen(SERVER_PORT)
            .expect("broker can listen");
        client_stack
            .connect(
                client_handle,
                IpEndpoint::new(IpAddress::Ipv4(SERVER_IP), SERVER_PORT),
                CLIENT_PORT,
            )
            .expect("client can initiate a connection");

        let mut now_ms: i64 = 0;
        while client_stack.tcp(client_handle).state() != State::Established
            || server_stack.tcp(server_handle).state() != State::Established
        {
            client_stack.poll(&mut device_client, now_ms);
            server_stack.poll(&mut device_server, now_ms);
            now_ms += 10;
        }

        // A fresh, self-signed identity (see `test_kit::tls`'s module docs) shared by both sides
        // of this loopback: the client trusts its cert as the sole root, the broker presents it.
        let generated = test_kit::generate_bringup_identity();
        let root = CertificateDer::from(generated.cert_der.clone());
        let identity = SoftwareTlsIdentity::from_der(
            CertificateDer::from(generated.cert_der),
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(generated.key_der)),
        )
        .expect("a freshly generated identity always builds");
        let mut tls_client = TlsStage::new_client("broker.sentient-rs.local", root);
        let mut tls_broker =
            TlsStage::new_server(&identity).expect("bring-up identity always builds");
        let mut io_buf = [0u8; TLS_CHUNK];

        for _ in 0..MAX_POLLS {
            client_stack.poll(&mut device_client, now_ms);
            server_stack.poll(&mut device_server, now_ms);

            if client_stack.tcp(client_handle).can_recv() {
                let n = client_stack
                    .tcp(client_handle)
                    .recv_slice(&mut io_buf)
                    .expect("client TCP recv");
                tls_client
                    .feed_incoming(&io_buf[..n])
                    .expect("client TLS feed");
            }
            let n = tls_client.drain_outgoing(&mut io_buf);
            if n > 0 {
                client_stack
                    .tcp(client_handle)
                    .send_slice(&io_buf[..n])
                    .expect("client TCP send");
            }

            if server_stack.tcp(server_handle).can_recv() {
                let n = server_stack
                    .tcp(server_handle)
                    .recv_slice(&mut io_buf)
                    .expect("broker TCP recv");
                tls_broker
                    .feed_incoming(&io_buf[..n])
                    .expect("broker TLS feed");
            }
            let n = tls_broker.drain_outgoing(&mut io_buf);
            if n > 0 {
                server_stack
                    .tcp(server_handle)
                    .send_slice(&io_buf[..n])
                    .expect("broker TCP send");
            }

            now_ms += 10;
            if !tls_client.is_handshaking() && !tls_broker.is_handshaking() {
                break;
            }
        }
        assert!(
            !tls_client.is_handshaking() && !tls_broker.is_handshaking(),
            "TLS handshake should complete before the MQTT PUBLISH is sent"
        );

        let mut publish_packet = [0u8; 64];
        let publish_len = encode_publish(TOPIC, PAYLOAD, 0, None, &mut publish_packet)
            .expect("encode_publish should succeed");
        tls_client
            .write_plaintext(&publish_packet[..publish_len])
            .expect("client can queue the PUBLISH as TLS plaintext");

        let mut broker_plaintext = [0u8; 256];
        let mut broker_got = 0usize;
        for _ in 0..MAX_POLLS {
            client_stack.poll(&mut device_client, now_ms);
            server_stack.poll(&mut device_server, now_ms);

            let n = tls_client.drain_outgoing(&mut io_buf);
            if n > 0 {
                client_stack
                    .tcp(client_handle)
                    .send_slice(&io_buf[..n])
                    .expect("client TCP send");
            }
            if server_stack.tcp(server_handle).can_recv() {
                let n = server_stack
                    .tcp(server_handle)
                    .recv_slice(&mut io_buf)
                    .expect("broker TCP recv");
                tls_broker
                    .feed_incoming(&io_buf[..n])
                    .expect("broker TLS feed of the sealed PUBLISH record");
            }
            broker_got = tls_broker
                .read_plaintext(&mut broker_plaintext)
                .expect("broker can decrypt the PUBLISH record");
            if broker_got > 0 {
                break;
            }
            now_ms += 10;
        }
        assert!(
            broker_got > 0,
            "the broker should receive a decrypted PUBLISH packet"
        );

        let (topic, payload) = topic_and_payload_of_qos0_publish(&broker_plaintext[..broker_got]);
        assert_eq!(
            topic, TOPIC,
            "the broker should see the exact topic the client published"
        );
        assert_eq!(
            payload, PAYLOAD,
            "the broker should see the exact payload the client published"
        );
    }
}

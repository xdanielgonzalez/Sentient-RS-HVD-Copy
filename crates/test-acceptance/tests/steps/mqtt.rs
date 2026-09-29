//! Steps for the mqtt feature area — MQTT 3.1.1 wire-encoder / CONNACK-parser / session
//! state-machine / publish-contract spec-compliance, entirely in-process (no broker/socket; the
//! broker-delivery loopback lives in `test-e2e/tests/owned_mqtt.rs`).
//!
//! **No protocol/wire-format logic is defined in this file.** Every encode/parse/session-transition
//! operation below is a direct call into the real, reused crate: `lib_net::link::mqtt_client`
//! (`encode_connect`/`encode_publish`/`encode_pingreq`/`parse_connack`, MQTT 3.1.1 sections 3.1,
//! 3.3, 3.13, 3.2) and `lib_net::link::mqtt_session::MqttSession` (the CONNECT->CONNACK->PUBLISH
//! ordering + keep-alive state machine), plus `lib_mapping::sink` (the shared no_std publish
//! contract's `Publisher`/`publish_tms`/`topic_into`). What lives here is Gherkin<->Rust plumbing
//! only: staging Given-step field values, calling the one real API under test, and asserting on the
//! typed result — never re-deriving a packet layout, a state transition, or a bound.
//!
//! Test code only; freely uses unwrap/expect/panic (the workspace lint gate denies these in
//! production logic, not tests).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cucumber::{given, then, when};

use heapless::String as HString;
use heapless::Vec as HVec;
use lib_mapping::schema::{
    ActiveDiagnosticState, ClockMonotonic, DiagnosticTroubleCode, DtcSeverity,
    FailureModeIndicator, Identity, String32, MAX_DTC,
};
use lib_mapping::sink::{self, PublishError, Publisher};
use lib_net::link::mqtt_client::{
    encode_connect, encode_pingreq, encode_publish, parse_connack, ConnAck, ConnectOptions,
    MqttClientError,
};
use lib_net::link::mqtt_session::{MqttSession, SessionError};

use crate::steps::support::{error_variant_name, parse_hex_bytes};
use crate::SpecWorld;

/// Scenario state for the mqtt feature area.
///
/// `Debug` is hand-rolled (not derived) because [`MqttSession`] does not implement it.
#[derive(Default)]
pub struct MqttState {
    // ---- CONNECT input, staged by Given steps ----
    client_id: String,
    keep_alive_secs: u16,
    clean_session: bool,
    username: Option<String>,
    password: Option<Vec<u8>>,

    // ---- PUBLISH input, staged by Given steps ----
    topic: String,
    payload: Vec<u8>,
    qos: u8,
    packet_id: Option<u16>,
    /// `None` = use a generous scratch buffer; `Some(n)` = the "short out-buf" error scenario.
    out_buf_len: Option<usize>,

    // ---- Most recent encode (CONNECT/PUBLISH/PINGREQ) or CONNACK-parse outcome ----
    encoded: Vec<u8>,
    encode_error: Option<MqttClientError>,

    // ---- CONNACK parse ----
    connack_input: Vec<u8>,
    connack: Option<ConnAck>,

    // ---- MqttSession under test ----
    /// `'static`: every session scenario pins the same "dev01"/60s `ConnectOptions` as the
    /// CONNECT_VECTOR / `mqtt_session.rs` unit tests, so the borrowed fields are string literals —
    /// this lets the session live in `SpecWorld`, which (like every cucumber `World`) carries no
    /// lifetime parameter.
    session: Option<MqttSession<'static>>,
    session_out: Vec<u8>,
    session_error: Option<SessionError>,

    // ---- lib_mapping::sink publish-contract bounds ----
    device_id: String,
    built_topic: HString<128>,
    topic_error: Option<PublishError>,
    publish_error: Option<PublishError>,
}

impl core::fmt::Debug for MqttState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MqttState")
            .field("client_id", &self.client_id)
            .field("topic", &self.topic)
            .field("encoded", &self.encoded)
            .field("encode_error", &self.encode_error)
            .field("connack", &self.connack)
            .field("session_is_some", &self.session.is_some())
            .field("session_out", &self.session_out)
            .field("session_error", &self.session_error)
            .field("built_topic", &self.built_topic)
            .field("topic_error", &self.topic_error)
            .field("publish_error", &self.publish_error)
            .finish_non_exhaustive()
    }
}

/// A [`Publisher`] that accepts anything — the sink bounds scenarios all fail before `publish` is
/// ever reached (a `topic_into`/serialize error short-circuits `publish_tms`), so this stub never
/// needs to record what it received.
struct NullPublisher;

impl Publisher for NullPublisher {
    fn publish(&mut self, _topic: &str, _payload: &[u8]) -> Result<(), PublishError> {
        Ok(())
    }
    fn healthy(&self) -> bool {
        true
    }
}

/// One [`DiagnosticTroubleCode`] with every field pushed to its maximum wire size (max-width
/// integers, both `Option` fields populated, `hint` filled to its 32-byte cap) — used to build an
/// `ActiveDiagnosticState` whose JSON serialization exceeds `MAX_PAYLOAD`.
fn maxed_code() -> DiagnosticTroubleCode {
    let mut hint: String32 = HString::new();
    hint.push_str(&"x".repeat(32))
        .expect("32 bytes fits String32");
    DiagnosticTroubleCode {
        port_number: Some(i32::MAX),
        suspect_parameter: u32::MAX,
        failure_mode: FailureModeIndicator::ConditionExists,
        occurrence_count: u32::MAX,
        time_raised: ClockMonotonic {
            seconds: u32::MAX,
            nanoseconds: u32::MAX,
        },
        severity: DtcSeverity::Major,
        hint,
        estimate_time: Some(ClockMonotonic {
            seconds: u32::MAX,
            nanoseconds: u32::MAX,
        }),
        estimate_severity: Some(DtcSeverity::Major),
    }
}

/// An `ActiveDiagnosticState` carrying the maximum `MAX_DTC` (64) codes, each maxed via
/// [`maxed_code`] — its JSON serialization comfortably exceeds `lib_mapping::sink::MAX_PAYLOAD`
/// (8192 bytes) by roughly 3x, well past any float/int-formatting ambiguity margin.
fn max_active_diagnostic_state() -> ActiveDiagnosticState {
    let mut codes: HVec<DiagnosticTroubleCode, MAX_DTC> = HVec::new();
    for _ in 0..MAX_DTC {
        codes.push(maxed_code()).expect("push up to MAX_DTC codes");
    }
    let mut device_id: Identity = HString::new();
    device_id.push_str("dev").expect("fits Identity");
    ActiveDiagnosticState {
        device_id,
        codes,
        overflow: 0,
    }
}

/// Advance `world.mqtt.session` at `now_ms` with `incoming` bytes, staging the result into
/// `session_out`/`session_error`. Shared by the `Given ... has been polled ...` (background setup)
/// and `When ... is polled ...` (the step under test) phrasings.
fn poll_session(world: &mut SpecWorld, now_ms: i64, incoming: Vec<u8>) {
    let session = world
        .mqtt
        .session
        .as_mut()
        .expect("MQTT session initialized");
    let mut out = [0u8; 128];
    match session.poll(now_ms, &incoming, &mut out) {
        Ok(n) => {
            world.mqtt.session_out = out[..n].to_vec();
            world.mqtt.session_error = None;
        }
        Err(e) => {
            world.mqtt.session_out.clear();
            world.mqtt.session_error = Some(e);
        }
    }
}

// ---- Given: CONNECT input ----------------------------------------------------------------------

#[given(expr = "a MQTT CONNECT with client id {string}, keep-alive {int} s, clean session {word}")]
fn given_connect_opts(
    world: &mut SpecWorld,
    client_id: String,
    keep_alive: i64,
    clean_session: String,
) {
    world.mqtt.client_id = client_id;
    world.mqtt.keep_alive_secs = keep_alive as u16;
    world.mqtt.clean_session = match clean_session.as_str() {
        "true" => true,
        "false" => false,
        other => panic!("unknown clean-session word {other:?} in feature file"),
    };
}

#[given(expr = "the MQTT CONNECT has username {string} and password {string}")]
fn given_connect_auth(world: &mut SpecWorld, username: String, password: String) {
    world.mqtt.username = Some(username);
    world.mqtt.password = Some(password.into_bytes());
}

// ---- When: CONNECT ------------------------------------------------------------------------------

#[when("the MQTT CONNECT is encoded")]
fn when_encode_connect(world: &mut SpecWorld) {
    let opts = ConnectOptions {
        client_id: &world.mqtt.client_id,
        keep_alive_secs: world.mqtt.keep_alive_secs,
        clean_session: world.mqtt.clean_session,
        username: world.mqtt.username.as_deref(),
        password: world.mqtt.password.as_deref(),
    };
    let mut out = [0u8; 256];
    match encode_connect(&opts, &mut out) {
        Ok(n) => {
            world.mqtt.encoded = out[..n].to_vec();
            world.mqtt.encode_error = None;
        }
        Err(e) => {
            world.mqtt.encoded.clear();
            world.mqtt.encode_error = Some(e);
        }
    }
}

// ---- Given: PUBLISH input -------------------------------------------------------------------

#[given(expr = "a MQTT PUBLISH to topic {string} with payload {string}")]
fn given_publish(world: &mut SpecWorld, topic: String, payload: String) {
    world.mqtt.topic = topic;
    world.mqtt.payload = payload.into_bytes();
}

#[given(expr = "the MQTT PUBLISH has QoS {int}")]
fn given_publish_qos(world: &mut SpecWorld, qos: i64) {
    world.mqtt.qos = qos as u8;
}

#[given(expr = "the MQTT PUBLISH has packet id {int}")]
fn given_publish_packet_id(world: &mut SpecWorld, id: i64) {
    world.mqtt.packet_id = Some(id as u16);
}

#[given(expr = "the MQTT PUBLISH topic is {int} bytes long")]
fn given_publish_topic_len(world: &mut SpecWorld, n: i64) {
    world.mqtt.topic = "x".repeat(n as usize);
}

#[given(expr = "the MQTT PUBLISH output buffer is {int} bytes")]
fn given_publish_out_buf(world: &mut SpecWorld, n: i64) {
    world.mqtt.out_buf_len = Some(n as usize);
}

// ---- When: PUBLISH / PINGREQ ------------------------------------------------------------------

#[when("the MQTT PUBLISH is encoded")]
fn when_encode_publish(world: &mut SpecWorld) {
    let mut scratch = vec![0u8; world.mqtt.out_buf_len.unwrap_or(256)];
    match encode_publish(
        &world.mqtt.topic,
        &world.mqtt.payload,
        world.mqtt.qos,
        world.mqtt.packet_id,
        &mut scratch,
    ) {
        Ok(n) => {
            world.mqtt.encoded = scratch[..n].to_vec();
            world.mqtt.encode_error = None;
        }
        Err(e) => {
            world.mqtt.encoded.clear();
            world.mqtt.encode_error = Some(e);
        }
    }
}

#[when("the MQTT PINGREQ is encoded")]
fn when_encode_pingreq(world: &mut SpecWorld) {
    let mut out = [0u8; 8];
    let n = encode_pingreq(&mut out).expect("encode_pingreq always succeeds (fixed 2-byte packet)");
    world.mqtt.encoded = out[..n].to_vec();
    world.mqtt.encode_error = None;
}

// ---- Then: encoded bytes -------------------------------------------------------------------------

#[then(expr = "the MQTT encoded bytes equal hex {string}")]
fn then_encoded_equals_hex(world: &mut SpecWorld, hex: String) {
    assert_eq!(
        world.mqtt.encoded,
        parse_hex_bytes(&hex),
        "encoded bytes must match the MQTT 3.1.1 wire vector"
    );
}

#[then(expr = "the MQTT CONNECT flags byte equals hex {string}")]
fn then_connect_flags_byte(world: &mut SpecWorld, hex: String) {
    let expected = parse_hex_bytes(&hex);
    assert_eq!(expected.len(), 1, "connect-flags is a single byte");
    // Fixed header (2) + "MQTT" length-prefixed string (2+4=6) + Protocol Level (1) = byte index 9.
    let actual = world.mqtt.encoded[9];
    assert_eq!(
        actual, expected[0],
        "CONNECT flags byte: got {actual:#04X}, expected {:#04X}",
        expected[0]
    );
}

#[then(expr = "the MQTT encode error is {word}")]
fn then_encode_error(world: &mut SpecWorld, name: String) {
    let err = world
        .mqtt
        .encode_error
        .expect("expected an encode error, got a success");
    assert_eq!(error_variant_name(&err), name);
}

// ---- Given/When/Then: CONNACK parse -----------------------------------------------------------

#[given(expr = "MQTT CONNACK bytes {string}")]
fn given_connack_bytes(world: &mut SpecWorld, hex: String) {
    world.mqtt.connack_input = parse_hex_bytes(&hex);
}

#[when("the MQTT CONNACK is parsed")]
fn when_parse_connack(world: &mut SpecWorld) {
    match parse_connack(&world.mqtt.connack_input) {
        Ok(ack) => {
            world.mqtt.connack = Some(ack);
            world.mqtt.encode_error = None;
        }
        Err(e) => {
            world.mqtt.connack = None;
            world.mqtt.encode_error = Some(e);
        }
    }
}

#[then(expr = "the MQTT CONNACK session present is {word}")]
fn then_connack_session_present(world: &mut SpecWorld, expected: String) {
    let ack = world.mqtt.connack.expect("CONNACK parsed successfully");
    let expected = match expected.as_str() {
        "true" => true,
        "false" => false,
        other => panic!("unknown boolean word {other:?} in feature file"),
    };
    assert_eq!(ack.session_present, expected);
}

#[then(expr = "the MQTT CONNACK return code is {int}")]
fn then_connack_return_code(world: &mut SpecWorld, expected: i64) {
    let ack = world.mqtt.connack.expect("CONNACK parsed successfully");
    assert_eq!(ack.return_code, expected as u8);
}

// ---- Given/When/Then: MqttSession --------------------------------------------------------------

#[given(expr = "a MQTT session for client id {string} with keep-alive {int} s")]
fn given_mqtt_session(world: &mut SpecWorld, client_id: String, keep_alive: i64) {
    // Every session scenario pins the CONNECT_VECTOR / mqtt_session.rs unit tests' fixed "dev01"
    // client id so `ConnectOptions`'s borrowed fields can be `'static` literals (see `MqttState`'s
    // `session` field doc) — the feature file spells it out for readability, this just checks it.
    assert_eq!(
        client_id, "dev01",
        "session scenarios pin the CONNECT_VECTOR client id \"dev01\""
    );
    let opts = ConnectOptions {
        client_id: "dev01",
        keep_alive_secs: keep_alive as u16,
        clean_session: true,
        username: None,
        password: None,
    };
    world.mqtt.session = Some(MqttSession::new(opts));
}

#[given(expr = "the MQTT session has been polled at {int} ms with no incoming bytes")]
fn given_session_polled_empty(world: &mut SpecWorld, now_ms: i64) {
    poll_session(world, now_ms, Vec::new());
}

#[given(expr = "the MQTT session has been polled at {int} ms with incoming hex {string}")]
fn given_session_polled_hex(world: &mut SpecWorld, now_ms: i64, hex: String) {
    poll_session(world, now_ms, parse_hex_bytes(&hex));
}

#[when(expr = "the MQTT session is polled at {int} ms with no incoming bytes")]
fn when_session_polled_empty(world: &mut SpecWorld, now_ms: i64) {
    poll_session(world, now_ms, Vec::new());
}

#[when(expr = "the MQTT session is polled at {int} ms with incoming hex {string}")]
fn when_session_polled_hex(world: &mut SpecWorld, now_ms: i64, hex: String) {
    poll_session(world, now_ms, parse_hex_bytes(&hex));
}

#[when(expr = "the MQTT session publish is attempted with topic {string} and payload {string}")]
fn when_session_publish(world: &mut SpecWorld, topic: String, payload: String) {
    let session = world
        .mqtt
        .session
        .as_mut()
        .expect("MQTT session initialized");
    match session.publish(&topic, payload.as_bytes()) {
        Ok(()) => world.mqtt.session_error = None,
        Err(e) => world.mqtt.session_error = Some(e),
    }
}

#[then(expr = "the MQTT session output equals hex {string}")]
fn then_session_output_hex(world: &mut SpecWorld, hex: String) {
    assert_eq!(world.mqtt.session_out, parse_hex_bytes(&hex));
}

#[then("the MQTT session is connected")]
fn then_session_connected(world: &mut SpecWorld) {
    assert!(
        world
            .mqtt
            .session
            .as_ref()
            .expect("MQTT session initialized")
            .is_connected(),
        "expected the session to be connected"
    );
}

#[then("the MQTT session is not connected")]
fn then_session_not_connected(world: &mut SpecWorld) {
    assert!(
        !world
            .mqtt
            .session
            .as_ref()
            .expect("MQTT session initialized")
            .is_connected(),
        "expected the session to not be connected yet"
    );
}

#[then(expr = "the MQTT session error is {word}")]
fn then_session_error(world: &mut SpecWorld, name: String) {
    let err = world
        .mqtt
        .session_error
        .expect("expected a session error, got none");
    assert_eq!(error_variant_name(&err), name);
}

// ---- Given/When/Then: lib_mapping::sink publish-contract bounds --------------------------

#[given(expr = "a MQTT topic device id {string}")]
fn given_device_id(world: &mut SpecWorld, id: String) {
    world.mqtt.device_id = id;
}

#[given(expr = "a MQTT topic device id {int} bytes long")]
fn given_long_device_id(world: &mut SpecWorld, n: i64) {
    world.mqtt.device_id = "d".repeat(n as usize);
}

#[when(expr = "the MQTT topic is built with prefix {string} and tms topic {string}")]
fn when_build_topic(world: &mut SpecWorld, prefix: String, tms_topic: String) {
    let mut out: HString<128> = HString::new();
    match sink::topic_into(&prefix, &world.mqtt.device_id, &tms_topic, &mut out) {
        Ok(()) => {
            world.mqtt.built_topic = out;
            world.mqtt.topic_error = None;
        }
        Err(e) => {
            world.mqtt.built_topic.clear();
            world.mqtt.topic_error = Some(e);
        }
    }
}

#[then(expr = "the MQTT built topic equals {string}")]
fn then_built_topic(world: &mut SpecWorld, expected: String) {
    assert_eq!(world.mqtt.built_topic.as_str(), expected.as_str());
}

#[then(expr = "the MQTT topic build error is {word}")]
fn then_topic_build_error(world: &mut SpecWorld, name: String) {
    let err = world
        .mqtt
        .topic_error
        .expect("expected a topic build error, got none");
    assert_eq!(error_variant_name(&err), name);
}

#[when("a maximally-sized ActiveDiagnosticState TMS payload is published via lib_mapping::sink")]
fn when_publish_max_payload(world: &mut SpecWorld) {
    let diag = max_active_diagnostic_state();
    let mut pubr = NullPublisher;
    match sink::publish_tms(
        &mut pubr,
        sink::DEFAULT_PREFIX,
        "dev",
        lib_mapping::schema::topics::ACTIVE_DIAGNOSTIC_STATE,
        &diag,
    ) {
        Ok(()) => world.mqtt.publish_error = None,
        Err(e) => world.mqtt.publish_error = Some(e),
    }
}

#[then(expr = "the MQTT publish error is {word}")]
fn then_publish_error(world: &mut SpecWorld, name: String) {
    let err = world
        .mqtt
        .publish_error
        .expect("expected a publish error, got none");
    assert_eq!(error_variant_name(&err), name);
}

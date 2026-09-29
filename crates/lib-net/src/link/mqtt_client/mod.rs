//! An owned MQTT 3.1.1 client: wire encoders for CONNECT/PUBLISH/PINGREQ and a CONNACK parser,
//! byte-exact to the OASIS MQTT v3.1.1 spec (<https://docs.oasis-open.org/mqtt/mqtt/v3.1.1/>).
//!
//! Sits above [`crate::link::tls::TlsStage`] (feature `tls`) or directly over a [`crate::link::NetStack`] TCP
//! socket for a plaintext broker — replacing the rumqttd/mosquitto client path with an owned,
//! no_std-safe encoder (caller-owned fixed buffers, no heap) that fits the MCU build. Kept
//! independent of `std`/`tls`: encoding/parsing MQTT control packets is pure byte manipulation and
//! needs neither.

mod prelude;
mod types;

pub use types::*;

use prelude::*;

/// Encode a CONNECT packet (MQTT 3.1.1 section 3.1) into `out`, returning the number of bytes
/// written.
pub fn encode_connect(opts: &ConnectOptions<'_>, out: &mut [u8]) -> Result<usize, MqttClientError> {
    let client_id = opts.client_id.as_bytes();
    if client_id.len() > u16::MAX as usize {
        return Err(MqttClientError::FieldTooLong);
    }
    if let Some(u) = opts.username {
        if u.len() > u16::MAX as usize {
            return Err(MqttClientError::FieldTooLong);
        }
    }
    if let Some(p) = opts.password {
        if p.len() > u16::MAX as usize {
            return Err(MqttClientError::FieldTooLong);
        }
    }

    // Variable header: "MQTT" (2-byte len + 4 bytes) + level (1) + flags (1) + keep-alive (2) = 10.
    let mut remaining = 10usize + 2 + client_id.len();
    if let Some(u) = opts.username {
        remaining += 2 + u.len();
    }
    if let Some(p) = opts.password {
        remaining += 2 + p.len();
    }

    let mut connect_flags = 0u8;
    if opts.username.is_some() {
        connect_flags |= 0x80;
    }
    if opts.password.is_some() {
        connect_flags |= 0x40;
    }
    if opts.clean_session {
        connect_flags |= 0x02;
    }

    let mut pos = 0usize;
    put_u8(out, &mut pos, 0x10)?; // fixed header: packet type CONNECT, flags 0000
    put_remaining_length(out, &mut pos, remaining)?;
    put_str_field(out, &mut pos, b"MQTT")?;
    put_u8(out, &mut pos, 0x04)?; // Protocol Level: 4 = MQTT 3.1.1
    put_u8(out, &mut pos, connect_flags)?;
    put_u16(out, &mut pos, opts.keep_alive_secs)?;
    put_str_field(out, &mut pos, client_id)?;
    if let Some(u) = opts.username {
        put_str_field(out, &mut pos, u.as_bytes())?;
    }
    if let Some(p) = opts.password {
        put_str_field(out, &mut pos, p)?;
    }
    Ok(pos)
}

/// Encode a PUBLISH packet (MQTT 3.1.1 section 3.3) for `topic`/`payload` into `out`. QoS 1/2
/// require a `packet_id`; QoS 0 must pass `None`.
pub fn encode_publish(
    topic: &str,
    payload: &[u8],
    qos: u8,
    packet_id: Option<u16>,
    out: &mut [u8],
) -> Result<usize, MqttClientError> {
    let topic_bytes = topic.as_bytes();
    if topic_bytes.len() > u16::MAX as usize {
        return Err(MqttClientError::FieldTooLong);
    }
    if qos > 2 {
        return Err(MqttClientError::Malformed);
    }
    if qos > 0 && packet_id.is_none() {
        return Err(MqttClientError::MissingPacketId);
    }

    let mut remaining = 2 + topic_bytes.len() + payload.len();
    if qos > 0 {
        remaining += 2;
    }

    let mut pos = 0usize;
    put_u8(out, &mut pos, 0x30 | (qos << 1))?; // fixed header: PUBLISH, DUP=0, RETAIN=0
    put_remaining_length(out, &mut pos, remaining)?;
    put_str_field(out, &mut pos, topic_bytes)?;
    if let Some(id) = packet_id {
        if qos > 0 {
            put_u16(out, &mut pos, id)?;
        }
    }
    put_bytes(out, &mut pos, payload)?;
    Ok(pos)
}

/// Encode a PINGREQ packet (MQTT 3.1.1 section 3.13 — fixed 2 bytes: `0xC0 0x00`) into `out`.
pub fn encode_pingreq(out: &mut [u8]) -> Result<usize, MqttClientError> {
    let mut pos = 0usize;
    put_u8(out, &mut pos, 0xC0)?;
    put_u8(out, &mut pos, 0x00)?;
    Ok(pos)
}

/// Parse a CONNACK packet (MQTT 3.1.1 section 3.2) from `bytes` — a single, complete packet; framing
/// above a byte stream belongs to the transport, mirroring how `lib-channel`'s boundary units
/// are already whole messages.
pub fn parse_connack(bytes: &[u8]) -> Result<ConnAck, MqttClientError> {
    let [type_and_flags, remaining_length, ack_flags, return_code] = bytes else {
        return Err(MqttClientError::Malformed);
    };
    if *type_and_flags != 0x20 || *remaining_length != 0x02 {
        return Err(MqttClientError::Malformed);
    }
    // Only bit 0 (Session Present) is defined (MQTT 3.1.1 section 3.2.2.1); the rest must be 0.
    if ack_flags & 0xFE != 0 {
        return Err(MqttClientError::Malformed);
    }
    Ok(ConnAck {
        session_present: ack_flags & 0x01 != 0,
        return_code: *return_code,
    })
}

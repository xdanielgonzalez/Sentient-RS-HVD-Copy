//! TMS Request/Reply control path (a `Reply` within 0.1 s of any request) — see the gap analysis's
//! "Request/Reply control path" finding.
//!
//! DEVICE ROLE DECISION (locked, see `docs/tms-compliance.md`): this adapter proxies a STOR-role
//! power device that implements no command surface yet, so it answers a well-formed request with
//! `ReplyCode::NotImplemented` and an undecodable payload with `ReplyCode::BadRequest` — both
//! within the 0.1 s budget. TopicInfo.supported_request_topics stays EMPTY (the honest
//! declaration); the two named request-topic subscriptions below are defense-in-depth so a request
//! addressed to us never goes unanswered.
//!
//! **Per identity (D13):** like `tasks::heartbeat`, `crate::runtime::supervise` spawns ONE
//! `run_reply_listener` PER derived [`IdentityCfg`] with INJECTED params (the identity + broker
//! host:port) — no `Config` reload. It opens its own loopback `rumqttc` client (subscribe + publish
//! over the one connection; identity-suffixed client-id `{device_id}-reply-listener` so N identities
//! never collide) — the documented fallback (see `lib_mqtt::broker::impl_broker_subscriber`).

use std::time::Duration;

use lib_mapping::schema::{topics, EnergyStartStopRequest, PowerSwitchRequest, Reply};
use rumqttc::{AsyncClient, Event, Incoming, MqttOptions, QoS};

use crate::config::IdentityCfg;
use crate::tasks::request_reply::{self, RequestKind};
use crate::tasks::util::bounded;

/// MQTT keep-alive for this task's own client connection.
const KEEP_ALIVE_SECS: u64 = 30;

/// Encode `reply` and publish it to `tms/{device_id}/reply` at QoS AtLeastOnce (per the standard's
/// Reply QoS profile), non-blocking. Failures are logged, never propagated.
fn publish_reply(client: &AsyncClient, device_id: &str, reply: &Reply) {
    let mut topic: heapless::String<{ lib_mapping::sink::MAX_TOPIC }> = heapless::String::new();
    if lib_mapping::sink::topic_into("tms", device_id, topics::REPLY, &mut topic).is_err() {
        log::warn!("reply_listener: reply topic build failed (device id too long?)");
        return;
    }
    let mut buf = [0u8; lib_mapping::sink::MAX_PAYLOAD];
    let n = match lib_mapping::render::render_value(reply, &mut buf) {
        Ok(n) => n,
        Err(e) => {
            log::warn!("reply_listener: JSON encode failed for Reply: {e}");
            return;
        }
    };
    if let Err(e) = client.try_publish(topic.as_str(), QoS::AtLeastOnce, false, &buf[..n]) {
        log::warn!("reply_listener: publish failed for Reply: {e}");
    }
}

/// Build the `Reply` to a request that arrived on `kind`'s topic: decode the MQTT/JSON payload,
/// then hand off to the transport-neutral core (`crate::tasks::request_reply`) — the ONLY thing
/// specific to this transport is the JSON decode step. An undecodable payload gets
/// `ReplyCode::BadRequest`.
fn build_reply(device_id: &heapless::String<32>, kind: RequestKind, payload: &[u8]) -> Reply {
    match kind {
        RequestKind::EnergyStartStop => {
            match serde_json_core::from_slice::<EnergyStartStopRequest>(payload) {
                Ok((req, _)) => request_reply::energy_start_stop_reply(device_id, &req),
                Err(_) => request_reply::bad_request(device_id.clone()),
            }
        }
        RequestKind::PowerSwitch => {
            match serde_json_core::from_slice::<PowerSwitchRequest>(payload) {
                Ok((req, _)) => request_reply::power_switch_reply(device_id, &req),
                Err(_) => request_reply::bad_request(device_id.clone()),
            }
        }
    }
}

/// Listen for inbound TMS requests on the named request topics and publish a `Reply` within the
/// standard's 0.1 s deadline.
pub async fn run_reply_listener(
    identity: IdentityCfg,
    broker_host: String,
    broker_port: u16,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    let device_id: heapless::String<32> = bounded(&identity.device_id);
    let mut opts = MqttOptions::new(
        format!("{device_id}-reply-listener"),
        broker_host,
        broker_port,
    );
    opts.set_keep_alive(Duration::from_secs(KEEP_ALIVE_SECS));
    let (client, mut eventloop) = AsyncClient::new(opts, 10);

    // Subscribe the two NAMED request topics (defense-in-depth; supported_request_topics stays
    // empty). Build each concrete topic string once for the inbound match.
    let mut energy_topic: heapless::String<{ lib_mapping::sink::MAX_TOPIC }> =
        heapless::String::new();
    let mut switch_topic: heapless::String<{ lib_mapping::sink::MAX_TOPIC }> =
        heapless::String::new();
    if lib_mapping::sink::topic_into(
        "tms",
        &device_id,
        topics::ENERGY_START_STOP_REQUEST,
        &mut energy_topic,
    )
    .is_err()
        || lib_mapping::sink::topic_into(
            "tms",
            &device_id,
            topics::POWER_SWITCH_REQUEST,
            &mut switch_topic,
        )
        .is_err()
    {
        log::error!("reply_listener: request topic build failed (device id too long?); disabled");
        let _ = shutdown.changed().await;
        return;
    }
    for t in [energy_topic.as_str(), switch_topic.as_str()] {
        if let Err(e) = client.subscribe(t, QoS::AtLeastOnce).await {
            log::error!("reply_listener: subscribe to {t} failed: {e}; disabled");
            let _ = shutdown.changed().await;
            return;
        }
    }

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    break;
                }
            }
            event = eventloop.poll() => {
                match event {
                    Ok(Event::Incoming(Incoming::Publish(p))) => {
                        let kind = if p.topic == energy_topic.as_str() {
                            Some(RequestKind::EnergyStartStop)
                        } else if p.topic == switch_topic.as_str() {
                            Some(RequestKind::PowerSwitch)
                        } else {
                            None
                        };
                        if let Some(kind) = kind {
                            let reply = build_reply(&device_id, kind, &p.payload);
                            publish_reply(&client, &device_id, &reply);
                        }
                    }
                    Ok(_) => {}
                    Err(e) => {
                        log::debug!("reply_listener: mqtt eventloop error (will retry): {e}");
                        tokio::time::sleep(Duration::from_millis(20)).await;
                    }
                }
            }
        }
    }
}

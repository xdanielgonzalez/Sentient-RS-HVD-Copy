//! Shared helpers for the per-identity background tasks (D13): the heartbeat/DeviceInfo task and
//! the Request/Reply listener. Formerly each task file carried its own copy of `bounded` (and
//! `heartbeat.rs` its own `publish_json`); consolidated here now that both tasks are spawned once
//! PER IDENTITY with injected params (no `Config` reload), so a single shared helper is the natural
//! home rather than two divergent copies.

use lib_tms::fields::HasChain;
use rumqttc::{AsyncClient, QoS};

/// Truncate `s` to `N` chars into a bounded heapless string (mirrors `runtime.rs`'s `bounded_str`).
pub fn bounded<const N: usize>(s: &str) -> heapless::String<N> {
    let end = s.char_indices().nth(N).map_or(s.len(), |(i, _)| i);
    let mut out = heapless::String::new();
    let _ = out.push_str(&s[..end.min(s.len())]);
    out
}

/// Encode `payload` via the flat chain serializer and publish it to
/// `tms/{device_id}/{snake_case(tms_topic)}` at QoS AtMostOnce (not retained), non-blocking.
/// Failures are logged, never propagated — a missed Heartbeat publish must not stall or crash the
/// task. Thin wrapper over [`publish_json_opts`] for the periodic best-effort topics (Heartbeat).
pub fn publish_json<T: HasChain>(
    client: &AsyncClient,
    device_id: &str,
    tms_topic: &str,
    payload: &T,
) {
    publish_json_opts(
        client,
        device_id,
        tms_topic,
        payload,
        QoS::AtMostOnce,
        false,
    );
}

/// Like [`publish_json`], but with an explicit `qos` and `retain`. Used for `DeviceInfo`, whose
/// spec profile is PublishLast / TRANSIENT_LOCAL / RELIABLE (IDL 28-36; MIL-STD-3071 B.5.1 / D.7):
/// published retained + QoS AtLeastOnce over the task's own loopback `rumqttc` client so a late
/// subscriber gets the current `DeviceInfo` immediately (the retain limitation noted for the
/// in-process broker `LinkTx` does NOT apply to this TCP client path). The 60 s periodic re-publish
/// is kept as belt-and-suspenders durability. Non-blocking; failures are logged, never propagated.
pub fn publish_json_opts<T: HasChain>(
    client: &AsyncClient,
    device_id: &str,
    tms_topic: &str,
    payload: &T,
    qos: QoS,
    retain: bool,
) {
    let mut topic: heapless::String<{ lib_mapping::sink::MAX_TOPIC }> = heapless::String::new();
    if lib_mapping::sink::topic_into("tms", device_id, tms_topic, &mut topic).is_err() {
        log::warn!("task: topic build failed for {tms_topic} (device id too long?)");
        return;
    }
    let mut buf = [0u8; lib_mapping::sink::MAX_PAYLOAD];
    let n = match lib_mapping::render::render_value(payload, &mut buf) {
        Ok(n) => n,
        Err(e) => {
            log::warn!("task: JSON encode failed for {tms_topic}: {e}");
            return;
        }
    };
    if let Err(e) = client.try_publish(topic.as_str(), qos, retain, &buf[..n]) {
        log::warn!("task: publish failed for {tms_topic}: {e}");
    }
}

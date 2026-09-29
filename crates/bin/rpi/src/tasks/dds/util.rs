//! Shared helpers for the DDS-side background tasks (#324, WP6): render a `HasCdrChain` topic
//! struct to XCDR2 and enqueue it as a [`DdsOutbound`] on the shared [`DdsHandle`]. Mirrors
//! `tasks::util`'s role for the MQTT tasks (`publish_json`/`publish_json_opts`) — non-blocking,
//! failures logged and never propagated.

use lib_dds::{DdsHandle, DdsOutbound};
use lib_mapping::render_cdr::render_cdr_value;
use lib_mapping::schema::Identity;
use lib_tms::cdr::HasCdrChain;
use lib_tms::TmsTopicId;

/// Enqueue already-rendered XCDR2 `payload` as a [`DdsOutbound`] for `topic`. Non-blocking;
/// dropped-and-logged on a full/closed queue.
pub fn send(handle: &DdsHandle, device_id: &Identity, topic: TmsTopicId, payload: Vec<u8>) {
    let out = DdsOutbound {
        device_id: device_id.clone(),
        topic,
        payload,
    };
    if handle.try_send(out).is_err() {
        log::debug!("dds task: queue full or stack gone; {topic:?} sample dropped");
    }
}

/// Render `value` (any [`HasCdrChain`] topic struct) into a fresh buffer and enqueue it via
/// [`send`]. Sized at `lib_mapping::sink::MAX_PAYLOAD` — generously oversized for XCDR2 (the JSON
/// encoding of the same topics is the larger of the two; observed CDR frames top out around
/// 160 B).
pub fn render_and_send<T: HasCdrChain>(
    handle: &DdsHandle,
    device_id: &Identity,
    topic: TmsTopicId,
    value: &T,
) {
    let mut buf = [0u8; lib_mapping::sink::MAX_PAYLOAD];
    match render_cdr_value(value, &mut buf) {
        Ok(n) => send(handle, device_id, topic, buf[..n].to_vec()),
        Err(e) => log::warn!("dds task: XCDR2 render failed for {topic:?}: {e}"),
    }
}

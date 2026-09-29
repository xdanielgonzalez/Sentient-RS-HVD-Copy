//! [`LatestStore`] — a bounded topic→latest-payload store for the device-hosted MQTT broker.

use heapless::{String as HeaplessString, Vec as HeaplessVec};

/// Maximum number of distinct topics this broker can track for local device publishes (telemetry
/// emit). Once full, the oldest topic entry (LRU) is evicted to make room for a new one.
pub const DEFAULT_MAX_TOPICS: usize = 32;

/// Default maximum payload bytes (already serialized, rendered on emit).
pub const DEFAULT_MAX_PAYLOAD_LEN: usize = 8192;

/// The topic string max length (matches [`crate::channel::message::MQTT_TOPIC`] — device publishes
/// into the bounded channel, so payloads are already bounded by the TMS topic limit).
pub(in crate::embassy::pump) const MAX_TOPIC_LEN: usize = 128;

/// A bounded store of topic → latest payload bytes. Used by the async broker pump to hold
/// canonical telemetry data in memory, rendered on output to subscribed clients (no payload
/// storage in the broker itself — it renders on demand from this store's copy).
///
/// `store.get(topic)` returns the latest payload if the topic exists, or `None` if it has never
/// been published. `store.upsert(topic, payload)` records or updates a topic's latest value, evicting
/// the oldest entry (LRU) if the store is full and the topic is new.
///
/// **no_std, no-alloc**: backed by `heapless::Vec`, so a fixed N-topic ceiling (per the `MAX_TOPICS`
/// const generic) and fixed per-topic payload capacity (per the `MAX_PAYLOAD_LEN` const generic).
/// Render loop uses the payload directly (no copy).
#[derive(Debug, Default)]
pub struct LatestStore<
    const MAX_TOPICS: usize = DEFAULT_MAX_TOPICS,
    const MAX_PAYLOAD_LEN: usize = DEFAULT_MAX_PAYLOAD_LEN,
> {
    /// Fixed-capacity table: (topic, payload_bytes). Entries are in insertion order; on upsert of
    /// a NEW topic when full, the entry at index 0 is removed (FIFO/LRU, not a perfect LRU, but
    /// simple and O(1) eviction — a real LRU would require bookkeeping). Upsert of an EXISTING
    /// topic (update) just mutates the payload in-place, no eviction.
    pub(in crate::embassy::pump) entries: HeaplessVec<
        (
            HeaplessString<MAX_TOPIC_LEN>,
            HeaplessVec<u8, MAX_PAYLOAD_LEN>,
        ),
        MAX_TOPICS,
    >,
}

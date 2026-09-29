//! `impl LatestStore` — bounded topic→latest-payload store operations.

use heapless::{String as HeaplessString, Vec as HeaplessVec};

use super::types::{LatestStore, StoreError};

impl<const MAX_TOPICS: usize, const MAX_PAYLOAD_LEN: usize>
    LatestStore<MAX_TOPICS, MAX_PAYLOAD_LEN>
{
    /// A fresh, empty store.
    pub const fn new() -> Self {
        LatestStore {
            entries: HeaplessVec::new(),
        }
    }

    /// Return the latest payload bytes for `topic`, or `None` if the topic has never been
    /// published.
    pub fn get(&self, topic: &str) -> Option<&[u8]> {
        self.entries
            .iter()
            .find(|(t, _)| t.as_str() == topic)
            .map(|(_, payload)| payload.as_slice())
    }

    /// Record or update `topic`'s latest payload. If the topic already exists, its payload is
    /// updated in-place. If it does NOT exist and the store is full, the oldest entry (index 0) is
    /// removed to make room (FIFO eviction). Returns `Err` if the topic or payload is oversized.
    pub fn upsert(&mut self, topic: &str, payload: &[u8]) -> Result<(), StoreError> {
        // Try to find and update an existing entry.
        if let Some((_t, existing_payload)) =
            self.entries.iter_mut().find(|(t, _)| t.as_str() == topic)
        {
            existing_payload.clear();
            existing_payload
                .extend_from_slice(payload)
                .map_err(|_| StoreError::PayloadTooLarge)?;
            return Ok(());
        }

        // Topic is new. If the store is full, evict the oldest (index 0).
        if self.entries.is_full() {
            self.entries.remove(0);
        }

        // Insert the new entry.
        let mut topic_str = HeaplessString::new();
        topic_str
            .push_str(topic)
            .map_err(|_| StoreError::TopicTooLong)?;

        let mut payload_vec = HeaplessVec::new();
        payload_vec
            .extend_from_slice(payload)
            .map_err(|_| StoreError::PayloadTooLarge)?;

        self.entries
            .push((topic_str, payload_vec))
            .map_err(|_| StoreError::PayloadTooLarge) // stack allocation failed, shouldn't happen
    }

    /// Clear all entries.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Number of entries currently in the store.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True if empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn upsert_and_get_new_topic() {
        let mut store = LatestStore::<4>::new();
        let payload = b"telemetry-001";

        assert!(store.get("t/reading").is_none());
        store.upsert("t/reading", payload).unwrap();
        assert_eq!(store.get("t/reading"), Some(payload.as_slice()));
    }

    #[test]
    fn upsert_updates_existing_topic() {
        let mut store = LatestStore::<4>::new();
        store.upsert("t/a", b"old").unwrap();
        store.upsert("t/a", b"new").unwrap();

        assert_eq!(store.get("t/a"), Some(b"new".as_slice()));
        assert_eq!(store.len(), 1); // still one entry
    }

    #[test]
    fn evicts_oldest_when_full() {
        let mut store = LatestStore::<2>::new();
        store.upsert("t/1", b"p1").unwrap();
        store.upsert("t/2", b"p2").unwrap();
        assert_eq!(store.len(), 2);

        // Store is full, new topic evicts oldest (t/1).
        store.upsert("t/3", b"p3").unwrap();
        assert_eq!(store.len(), 2);
        assert!(store.get("t/1").is_none()); // evicted
        assert_eq!(store.get("t/2"), Some(b"p2".as_slice()));
        assert_eq!(store.get("t/3"), Some(b"p3".as_slice()));
    }

    #[test]
    fn topic_too_long_fails() {
        let mut store = LatestStore::<4>::new();
        let long_topic = &"t".repeat(200); // exceeds MAX_TOPIC_LEN
        assert!(store.upsert(long_topic, b"p").is_err());
    }

    #[test]
    fn payload_too_large_fails() {
        let mut store = LatestStore::<4>::new();
        let large_payload = vec![0u8; 10000]; // exceeds MAX_PAYLOAD_LEN
        assert!(store.upsert("t/big", &large_payload).is_err());
    }
}

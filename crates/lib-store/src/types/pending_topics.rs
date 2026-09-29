use super::pending_topic::PendingTopic;

/// Up to 3 pending topics (the fixed set above) drained by [`crate::CanonicalStore::take_pending`] in a
/// single bounded burst — never allocates, never blocks.
pub type PendingTopics = heapless::Vec<PendingTopic, 3>;

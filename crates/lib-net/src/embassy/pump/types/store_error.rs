//! [`StoreError`] — the failure signal [`super::latest_store::LatestStore::upsert`] reports.

/// Error returned by [`super::latest_store::LatestStore::upsert`] if the topic or payload exceeds
/// size limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreError {
    /// The topic string exceeds the maximum length.
    TopicTooLong,
    /// The payload exceeds the maximum length.
    PayloadTooLarge,
}

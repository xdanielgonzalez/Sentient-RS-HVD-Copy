/// A message could not be enqueued.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrySendError<T> {
    /// The channel is at capacity; the item is returned so the caller can retry / apply back-pressure.
    Full(T),
    /// The receiving end is gone; the item is returned.
    Closed(T),
}

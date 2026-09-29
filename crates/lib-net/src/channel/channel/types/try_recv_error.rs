/// A message could not be dequeued without blocking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TryRecvError {
    /// No message is ready.
    Empty,
    /// The sending end is gone and the channel is drained.
    Closed,
}

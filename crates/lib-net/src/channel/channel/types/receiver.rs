use super::try_recv_error::TryRecvError;

/// The consuming half of a typed channel.
#[allow(async_fn_in_trait)]
pub trait Receiver {
    /// The message type carried by this channel.
    type Item;
    /// Dequeue without waiting.
    fn try_recv(&mut self) -> Result<Self::Item, TryRecvError>;
    /// Await the next message; `None` once the channel is closed and drained.
    async fn recv(&mut self) -> Option<Self::Item>;
}

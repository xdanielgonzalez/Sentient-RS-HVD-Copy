use super::closed::Closed;
use super::try_send_error::TrySendError;

/// The producing half of a typed channel.
#[allow(async_fn_in_trait)]
pub trait Sender {
    /// The message type carried by this channel (a boundary message).
    type Item;
    /// Enqueue without waiting. `Full`/`Closed` return the item so the caller keeps ownership and
    /// can decide (retry, back-pressure, drop-with-counter) — never a silent drop.
    fn try_send(&mut self, item: Self::Item) -> Result<(), TrySendError<Self::Item>>;
    /// Await capacity, then enqueue. Errors only if the channel is [`Closed`].
    async fn send(&mut self, item: Self::Item) -> Result<(), Closed>;
}

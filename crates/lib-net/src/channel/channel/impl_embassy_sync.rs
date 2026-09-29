//! `embassy-sync` 0.6.2 [`embassy_sync::channel::Channel`] impls of [`super::types::Sender`]/
//! [`super::types::Receiver`] — gated `#[cfg(feature = "channel-embassy")]`. This is the MCU
//! concrete channel: a fixed-capacity `heapless::Deque` behind a `RawMutex`, split into a
//! `Sender`/`Receiver` half that this module wires onto the crate's own channel contract so stage
//! code (`crate::channel::stage`) never names `embassy_sync` directly. `bin/mcu-rp2350` (story #352,
//! its own excluded workspace) enables this feature directly and instantiates
//! `bin-mcu-platform::ChannelPublisher<S: Sender<Item = MqttUnit>>` with the `Sender` half here —
//! `bin-mcu-platform` itself only ever names the `Sender`/`Receiver` traits, never this impl.
//!
//! An embassy `Channel` never closes (there is no receiver-dropped signal — both halves are
//! `'static`, borrowed from a shared `Channel`, and typically never dropped on an embedded
//! target), so the mapping is narrower than the general contract:
//! - [`TrySendError::Closed`]/[`TryRecvError::Closed`]/[`Closed`] are simply never produced —
//!   `try_send` can only return `Full`, `send` never errors, `try_recv` can only return `Empty`,
//!   and `recv` never returns `None`.
//! - `TrySendError::Full`/`TryRecvError::Empty` map straight across.

use super::types::{Closed, Receiver, Sender, TryRecvError, TrySendError};
use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::channel::{
    Receiver as EmbassyReceiver, Sender as EmbassySender, TryReceiveError as EmbassyTryRecvError,
    TrySendError as EmbassyTrySendError,
};

impl<'ch, M, T, const N: usize> Sender for EmbassySender<'ch, M, T, N>
where
    M: RawMutex,
{
    type Item = T;

    fn try_send(&mut self, item: T) -> Result<(), TrySendError<T>> {
        EmbassySender::try_send(self, item).map_err(|e| match e {
            EmbassyTrySendError::Full(item) => TrySendError::Full(item),
        })
    }

    /// Embassy channels never close, so this only ever awaits capacity — it cannot return `Err`.
    async fn send(&mut self, item: T) -> Result<(), Closed> {
        EmbassySender::send(self, item).await;
        Ok(())
    }
}

impl<'ch, M, T, const N: usize> Receiver for EmbassyReceiver<'ch, M, T, N>
where
    M: RawMutex,
{
    type Item = T;

    fn try_recv(&mut self) -> Result<T, TryRecvError> {
        EmbassyReceiver::try_receive(self).map_err(|e| match e {
            EmbassyTryRecvError::Empty => TryRecvError::Empty,
        })
    }

    /// Embassy channels never close, so this only ever awaits the next item — it cannot return
    /// `None`.
    async fn recv(&mut self) -> Option<T> {
        Some(EmbassyReceiver::receive(self).await)
    }
}

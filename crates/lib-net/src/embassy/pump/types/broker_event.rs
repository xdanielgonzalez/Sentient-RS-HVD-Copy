//! [`BrokerEvent`] — lifecycle events emitted by the device-hosted MQTT broker. defmt-free;
//! the bin supplies the defmt sink.

/// An event the async broker pump emits: client lifecycle, topic publish, link up/down, etc.
/// The caller provides a callback (`FnMut(BrokerEvent)`) and the pump calls it at key moments.
/// No payload — events are named states, keeping the event loop lean.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrokerEvent {
    /// Broker task started and listening on `:1883`.
    BrokerUp,
    /// A client connected (slot index included if needed via a separate callback, or logged by
    /// the sink directly).
    ClientConnect,
    /// A client disconnected.
    ClientClose,
    /// A topic was published (by a local device publisher).
    TopicPublish,
    /// Link went up.
    LinkUp,
    /// Link went down.
    LinkDown,
}

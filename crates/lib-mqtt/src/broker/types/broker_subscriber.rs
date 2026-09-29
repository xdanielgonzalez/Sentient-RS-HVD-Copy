use rumqttd::local::LinkRx;

/// A subscriber over the embedded broker's in-process Link — the `LinkRx` counterpart to
/// [`super::broker_publisher::BrokerPublisher`]'s `LinkTx`. Lets in-process consumers (e.g. a
/// future Request/Reply listener — see `docs/gap-analysis-meteor-parity.md`'s "Request/Reply
/// control path" finding) receive publishes without a loopback TCP client, IF `LinkRx` actually
/// delivers them this way — see the module docs' SPIKE note.
///
/// STUB (gap-closure scaffolding, #237): type only; [`LocalBroker::start`](super::local_broker::LocalBroker)
/// still drops its own `_link_rx` entirely today (see `impl_local_broker.rs`) — wiring a real
/// `BrokerSubscriber` through requires keeping that `LinkRx` alive instead, which is exactly what
/// this session's spike test (`tests/link_rx_spike.rs`) probes.
pub struct BrokerSubscriber {
    pub link_rx: LinkRx,
}

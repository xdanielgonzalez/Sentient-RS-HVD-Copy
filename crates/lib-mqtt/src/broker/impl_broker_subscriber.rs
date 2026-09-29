use super::prelude::*;

impl BrokerSubscriber {
    /// Poll for the next inbound publish delivered on this subscriber's `LinkRx`, bounded by
    /// `timeout`. Confirmed working by this session's spike (`tests/link_rx_spike.rs`,
    /// `link_rx_delivers_publishes_made_on_the_same_link` — run for real against rumqttd 0.19 once
    /// this session had registry access, not just recollection): `LinkRx::recv`/`recv_deadline`
    /// delivers every `Notification` posted to the connection this `LinkRx` belongs to, in order —
    /// including non-`Forward` notifications (`SubAck`, etc., e.g. from an earlier `subscribe`
    /// call), which this loops past rather than surfacing to the caller.
    ///
    /// Returns `Ok(None)` on a plain timeout (nothing arrived within `timeout`) or a clean
    /// disconnect; `Ok(Some(_))` on the next inbound publish; `Err(_)` only on an unexpected
    /// channel error (rumqttd's own `LinkError` is large — `-D clippy::result_large_err` — so it is
    /// mapped to the crate's existing [`NetError`], the same error type [`super::LocalBroker::start`]
    /// already surfaces for link-layer failures).
    ///
    /// NOT currently wired to [`super::LocalBroker`]: handing out a live `BrokerSubscriber` there
    /// requires `LocalBroker` to keep the `LinkRx` its `start()` mints alive (today it is dropped —
    /// see `impl_local_broker.rs`'s doc comment), which means adding a field to
    /// `types/local_broker.rs` — a file outside this workstream's owned-file boundary (see the
    /// #240/#244/#246 handoff). `bin-rpi::tasks::reply_listener` therefore uses the documented,
    /// equally-valid loopback `rumqttc` TCP client fallback for this workstream instead; this type
    /// is left fully implemented (spike-proven) for whichever follow-up work stream picks up the
    /// `LocalBroker` field addition.
    pub fn try_recv(&mut self, timeout: Duration) -> Result<Option<SubscribedMessage>, NetError> {
        let deadline = Instant::now() + timeout;
        loop {
            if Instant::now() >= deadline {
                return Ok(None);
            }
            match self.link_rx.recv_deadline(deadline) {
                Ok(Some(Notification::Forward(fwd))) => {
                    return Ok(Some(SubscribedMessage {
                        topic: String::from_utf8_lossy(&fwd.publish.topic).into_owned(),
                        payload: fwd.publish.payload.to_vec(),
                    }));
                }
                // Any other notification (SubAck, PubAck, ...) isn't a message for the caller —
                // keep polling until a real publish arrives or the deadline passes.
                Ok(Some(_other)) => continue,
                Ok(None) => return Ok(None),
                Err(LinkError::RecvTimeout(_)) => return Ok(None),
                Err(e) => return Err(NetError::Link(e.to_string())),
            }
        }
    }
}

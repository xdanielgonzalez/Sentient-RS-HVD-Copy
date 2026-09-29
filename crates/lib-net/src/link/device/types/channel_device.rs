/// A smoltcp device backed by an egress [`crate::channel::Sender`] and an ingress [`crate::channel::Receiver`] of [`crate::channel::WireFrame`]s.
pub struct ChannelDevice<Tx, Rx> {
    pub(crate) egress: Tx,
    pub(crate) ingress: Rx,
    pub(crate) mtu: usize,
}

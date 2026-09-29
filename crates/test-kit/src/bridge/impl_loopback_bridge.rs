use super::prelude::*;

impl LoopbackBridge {
    /// A new bridge carrying frames up to `mtu` bytes (see [`ChannelDevice::new`]).
    pub fn new(mtu: usize) -> Self {
        LoopbackBridge { mtu }
    }

    /// Split into two cross-wired [`ChannelDevice`]s ready to hand to two
    /// [`lib_net::link::NetStack`]s — `(device_a, device_b)`, where anything `device_a` transmits,
    /// `device_b` receives, and vice versa.
    pub fn split(
        self,
    ) -> (
        ChannelDevice<BridgeEnd, BridgeEnd>,
        ChannelDevice<BridgeEnd, BridgeEnd>,
    ) {
        // Two independent directional queues: whatever `a` transmits lands in `a_to_b` (which `b`
        // reads as ingress); whatever `b` transmits lands in `b_to_a` (which `a` reads as ingress).
        let a_to_b = BridgeEnd::new();
        let b_to_a = BridgeEnd::new();
        let device_a = ChannelDevice::new(a_to_b.clone(), b_to_a.clone(), self.mtu);
        let device_b = ChannelDevice::new(b_to_a, a_to_b, self.mtu);
        (device_a, device_b)
    }
}

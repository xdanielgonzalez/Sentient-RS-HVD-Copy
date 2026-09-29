/// A bridged pair: two [`lib_net::link::ChannelDevice`]s cross-wired so device `a`'s egress is device `b`'s
/// ingress and vice versa — the minimum needed to run two independent [`lib_net::link::NetStack`]s
/// against each other over an in-memory link instead of real hardware.
pub struct LoopbackBridge {
    pub mtu: usize,
}

use crate::channel::WireFrame;
use crate::link::LinkError;

/// A raw L2 Ethernet link — the driver behind the [`WireFrame`] boundary. Implementors move frames
/// to/from the hardware; everything above L2 (IP/TCP/TLS/MQTT) is owned and platform-free.
#[allow(async_fn_in_trait)]
pub trait NetLink {
    /// Receive one ingress frame, filling `out` (its `dir` is set to [`crate::channel::FlowDir::Ingress`]). Awaits
    /// a frame. `Err(LinkError::Down)` means the link is gone.
    async fn recv(&mut self, out: &mut WireFrame) -> Result<(), LinkError>;
    /// Transmit one egress frame.
    async fn send(&mut self, frame: &WireFrame) -> Result<(), LinkError>;
    /// The link's MAC address (for the owned IP stack).
    fn mac(&self) -> [u8; 6];
}

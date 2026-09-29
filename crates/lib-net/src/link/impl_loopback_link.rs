use crate::channel::{FlowDir, WireFrame};
use crate::link::{LinkError, LoopbackLink, NetLink};

impl<const N: usize> LoopbackLink<N> {
    /// A loopback link with the given MAC.
    pub fn new(mac: [u8; 6]) -> Self {
        LoopbackLink {
            queue: heapless::Deque::new(),
            mac,
        }
    }
    /// Preload a frame's bytes as if received (test helper). Fails closed if the queue is full.
    pub fn preload(&mut self, bytes: &[u8]) -> Result<(), LinkError> {
        let frame = WireFrame::new(FlowDir::Ingress, bytes).ok_or(LinkError::Overrun)?;
        self.queue.push_back(frame).map_err(|_| LinkError::Overrun)
    }
}

impl<const N: usize> NetLink for LoopbackLink<N> {
    async fn recv(&mut self, out: &mut WireFrame) -> Result<(), LinkError> {
        match self.queue.pop_front() {
            Some(frame) => {
                out.dir = FlowDir::Ingress;
                out.data.clear();
                out.data
                    .extend_from_slice(&frame.data)
                    .map_err(|_| LinkError::Overrun)?;
                Ok(())
            }
            None => Err(LinkError::Down),
        }
    }
    async fn send(&mut self, frame: &WireFrame) -> Result<(), LinkError> {
        let mut looped = WireFrame::empty(FlowDir::Ingress);
        looped
            .data
            .extend_from_slice(&frame.data)
            .map_err(|_| LinkError::Overrun)?;
        self.queue.push_back(looped).map_err(|_| LinkError::Overrun)
    }
    fn mac(&self) -> [u8; 6] {
        self.mac
    }
}

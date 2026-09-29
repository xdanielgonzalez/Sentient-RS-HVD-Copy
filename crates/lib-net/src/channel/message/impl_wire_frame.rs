use super::types::{FlowDir, WireFrame};

impl WireFrame {
    /// An empty frame in `dir` — a buffer a link fills on receive, or a builder for transmit.
    pub const fn empty(dir: FlowDir) -> Self {
        WireFrame {
            dir,
            data: heapless::Vec::new(),
        }
    }
    /// A frame carrying `bytes`; fails closed (`None`) if it exceeds the MTU.
    pub fn new(dir: FlowDir, bytes: &[u8]) -> Option<Self> {
        heapless::Vec::from_slice(bytes)
            .ok()
            .map(|data| WireFrame { dir, data })
    }
}

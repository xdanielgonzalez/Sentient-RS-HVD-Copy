use super::prelude::*;

impl<Tx> phy::TxToken for TxToken<'_, Tx>
where
    Tx: Sender<Item = WireFrame>,
{
    fn consume<R, F: FnOnce(&mut [u8]) -> R>(self, len: usize, f: F) -> R {
        let n = len.min(WIRE_MTU);
        let mut buf = [0u8; WIRE_MTU];
        let result = f(&mut buf[..n]);
        // Enqueue the built frame; L2 is lossy, so a full egress queue drops it (TCP retransmits).
        if let Some(frame) = WireFrame::new(FlowDir::Egress, &buf[..n]) {
            let _ = self.egress.try_send(frame);
        }
        result
    }
}

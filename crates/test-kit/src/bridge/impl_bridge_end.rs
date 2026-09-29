use super::prelude::*;

impl BridgeEnd {
    pub(super) fn new() -> Self {
        BridgeEnd(Rc::new(RefCell::new(VecDeque::new())))
    }
}

impl Sender for BridgeEnd {
    type Item = WireFrame;

    fn try_send(&mut self, item: WireFrame) -> Result<(), TrySendError<WireFrame>> {
        self.0.borrow_mut().push_back(item);
        Ok(())
    }

    async fn send(&mut self, item: WireFrame) -> Result<(), Closed> {
        self.0.borrow_mut().push_back(item);
        Ok(())
    }
}

impl Receiver for BridgeEnd {
    type Item = WireFrame;

    fn try_recv(&mut self) -> Result<WireFrame, TryRecvError> {
        self.0.borrow_mut().pop_front().ok_or(TryRecvError::Empty)
    }

    async fn recv(&mut self) -> Option<WireFrame> {
        self.0.borrow_mut().pop_front()
    }
}

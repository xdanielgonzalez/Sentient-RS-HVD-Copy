/// A transmit slot holding the egress sender; on consume it enqueues one frame.
pub struct TxToken<'a, Tx> {
    pub(crate) egress: &'a mut Tx,
}

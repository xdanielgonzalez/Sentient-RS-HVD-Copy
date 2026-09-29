use crate::channel::WireFrame;

/// A received frame, owned (moved out of the ingress channel) so it borrows nothing.
pub struct RxToken {
    pub(crate) frame: WireFrame,
}

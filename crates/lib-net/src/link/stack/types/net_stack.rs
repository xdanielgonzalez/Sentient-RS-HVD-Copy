use smoltcp::iface::{Interface, SocketSet};

/// The owned stack: a smoltcp [`Interface`] plus its fixed-capacity [`SocketSet`]. `'a` ties the
/// socket storage and buffers the caller supplies.
pub struct NetStack<'a> {
    pub(crate) iface: Interface,
    pub(crate) sockets: SocketSet<'a>,
}

use crate::channel::WireFrame;
use std::sync::mpsc as std_mpsc;
use tokio::sync::mpsc as tok_mpsc;

/// The Linux host L2 link over pnet/AF_PACKET, bound to one interface.
pub struct HostLink {
    pub(crate) ingress: tok_mpsc::Receiver<WireFrame>,
    pub(crate) egress: std_mpsc::SyncSender<WireFrame>,
    pub(crate) mac: [u8; 6],
}

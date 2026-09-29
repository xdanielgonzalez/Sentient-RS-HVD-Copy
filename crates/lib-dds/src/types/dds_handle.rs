use super::dds_outbound::DdsOutbound;
use tokio::sync::mpsc::Sender;

/// A clonable, bounded producer feeding the DDS stack (the poll-loop tap seam). Cheap to clone and
/// share across tasks; back-pressures on a full queue. Impl (send/try_send) in `impl_dds_handle.rs`;
/// paired with its `Receiver` by [`crate::dds_channel`].
#[derive(Debug, Clone)]
pub struct DdsHandle {
    pub(crate) tx: Sender<DdsOutbound>,
}

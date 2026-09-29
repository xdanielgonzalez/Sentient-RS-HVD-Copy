use super::rp2350_sender::Rp2350Sender;

/// This platform's `ChannelPublisher`, concrete over [`Rp2350Sender`].
pub(crate) type Rp2350Publisher = bin_mcu_platform::ChannelPublisher<Rp2350Sender>;

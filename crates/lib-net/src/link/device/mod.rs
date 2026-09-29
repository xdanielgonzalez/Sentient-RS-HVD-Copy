//! The smoltcp [`phy::Device`](smoltcp::phy::Device) bridge over the wire-byte channels.
//!
//! smoltcp is poll-driven: during a poll it pulls received frames and pushes frames to transmit.
//! `ChannelDevice` maps those onto the [`crate::channel::WireFrame`] channels via their *synchronous* `try_recv` /
//! `try_send` (poll runs to completion, it doesn't await), so the same device works over any
//! [`NetLink`](crate) driver behind those channels. no_std, no alloc — the tx scratch buffer is a
//! stack array bounded by the MTU.

mod prelude;
mod types;

pub use types::*;

mod impl_channel_device;
mod impl_rx_token;
mod impl_tx_token;

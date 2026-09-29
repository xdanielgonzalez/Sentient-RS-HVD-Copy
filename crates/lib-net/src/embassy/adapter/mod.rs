//! The **adapter**: bridges our [`crate::link::NetLink`] boundary to `embassy-net-driver-channel`'s
//! `Runner` — i.e. `link.recv()` -> the channel's RX queue, and the channel's TX queue ->
//! `link.send()`. This re-points `crate::link::pump_ingress`/`pump_egress`'s shape at
//! `embassy-net-driver-channel` instead of a [`crate::channel`] `Sender`/`Receiver` pair, so the SAME
//! `NetLink` driver (Zephyr NIC, W5500, [`crate::link::LoopbackLink`], or the host's
//! [`crate::link::host::HostLink`]) can sit underneath embassy-net's `Device`/`Runner` split instead
//! of (or alongside) the bespoke [`crate::link::stack::NetStack`] (smoltcp driven directly).
//!
//! Frames cross via plain byte copies through `RxRunner::rx_buf`/`rx_done` and
//! `TxRunner::tx_buf`/`tx_done` (see `embassy-net-driver-channel`'s docs) — no unsafe, no extra
//! buffering: one frame in flight per direction at a time, backpressured by the channel's queue
//! depth (`N_RX`/`N_TX` in [`embassy_net_driver_channel::State`]).

mod pumps;

pub use pumps::{pump_channel_to_link, pump_link_to_channel};

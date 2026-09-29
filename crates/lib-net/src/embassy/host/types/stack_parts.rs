//! [`StackParts`] — the pieces [`super::super::build_stack`] hands back.

use embassy_net::{Runner as NetRunner, Stack};
use embassy_net_driver_channel::Runner as ChannelRunner;

use crate::embassy::config::MTU;

/// The three handles a caller needs after [`super::build_stack`]: the `Stack` (for opening
/// sockets), the `embassy_net::Runner` (spawn `runner.run().await` as a task — advances
/// IP/TCP), and the driver-channel `Runner` (call [`embassy_net_driver_channel::Runner::split`]
/// then hand the `RxRunner`/`TxRunner` halves to [`crate::embassy::pump_link_to_channel`]/
/// [`crate::embassy::pump_channel_to_link`] tasks).
pub struct StackParts<'d> {
    /// Open TCP sockets against this.
    pub stack: Stack<'d>,
    /// Spawn as a task: drives the IP/TCP state machine.
    pub net_runner: NetRunner<'d, embassy_net_driver_channel::Device<'d, MTU>>,
    /// Split and hand to the two link-pump tasks.
    pub channel_runner: ChannelRunner<'d, MTU>,
}

//! Host-only bring-up building blocks (feature `host`): everything needed to stand up a real
//! `embassy_net::Stack` over the [`crate::embassy::adapter`] on the host, EXCEPT the
//! `#[embassy_executor::task]` wrapper functions themselves — `embassy-executor` tasks must be
//! concrete, non-generic functions (verified against `embassy-executor-macros`' `task.rs`: "task
//! functions must not be generic" / "tasks can't be generic"), so the task fns that close over a
//! specific [`crate::link::NetLink`] impl are necessarily monomorphized at the call site. See
//! `tests/host_bring_up.rs` for the concrete wiring (a two-peer in-memory link, an
//! `embassy-executor` `arch-std` executor on its own thread, and a real `TcpSocket` connect/accept
//! over it) — this module supplies the reusable, link-agnostic half: [`build_stack`] (a free
//! function — no type to split; see [`StackParts`] for the type it returns).

mod types;

pub use types::StackParts;

use crate::embassy::config::{StackConfig, MTU, N_RX, N_TX};
use embassy_net_driver_channel::State;

/// Build the driver-channel `Device`/`Runner` split plus the `embassy_net::Stack`/`Runner` pair
/// over it, from a [`StackConfig`] and caller-owned `'static` storage (the standard embassy
/// pattern: the caller promotes bring-up-time locals to `'static` once via
/// `static_cell::StaticCell` and passes `&'static mut` in, rather than this fn leaking anything
/// itself).
///
/// `stack_resources` sizes the socket table (`N` concurrent sockets) — the M1 proof needs exactly
/// one TCP socket per peer, so callers pass a `StackResources<1>`.
pub fn build_stack<'d, const N: usize>(
    config: &StackConfig,
    channel_state: &'d mut State<MTU, N_RX, N_TX>,
    stack_resources: &'d mut embassy_net::StackResources<N>,
    random_seed: u64,
) -> StackParts<'d> {
    let (channel_runner, device) = embassy_net_driver_channel::new(
        channel_state,
        embassy_net_driver::HardwareAddress::Ethernet(config.mac),
    );
    let (stack, net_runner) = embassy_net::new(
        device,
        config.embassy_config(),
        stack_resources,
        random_seed,
    );
    StackParts {
        stack,
        net_runner,
        channel_runner,
    }
}

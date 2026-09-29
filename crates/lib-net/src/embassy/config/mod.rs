//! The [`StackConfig`] the M1 host bring-up (and later the real Zephyr wiring) builds an
//! `embassy_net::Stack` from — a static IPv4 config plus the MTU/socket-count constants shared by
//! both the driver-channel adapter ([`crate::embassy::adapter`]) and the `embassy_net::Stack` itself, so the
//! two agree by construction rather than by convention.

mod impl_stack_config;
mod types;

pub use types::{StackConfig, MTU, N_RX, N_TX};

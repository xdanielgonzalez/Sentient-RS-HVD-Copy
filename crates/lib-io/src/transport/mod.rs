//! The byte-transport abstraction.
//!
//! One async trait for RS-232 / RS-485 serial. Callers are **generic over the transport**
//! (`<R: SerialTransport>`) — static dispatch, no `Box<dyn>`, no allocation, so the same driver
//! monomorphizes onto a host `tokio-serial` port or an embassy MCU UART. RS-485 half-duplex
//! direction control is part of the trait (a no-op on RS-232).

mod prelude;
mod types;

pub use types::*;

mod impl_serial_params;

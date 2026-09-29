//! UART hardware binding — see [`types::UartChannel`] for the seam this module exists to keep
//! thin. `impl_embassy_uart0.rs` is the phase-1 UART0 concrete implementation; UART1 (GP4/GP5) is
//! left unused/reserved for a phase-2 second responder node (see `hil/README.md`'s Phase-2
//! section).

mod impl_embassy_uart0;
mod types;

pub use types::{EmbassyUart0, UartChannel};

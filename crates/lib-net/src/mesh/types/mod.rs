// Base codec types — always compiled with the `mesh` feature (no_std, no alloc; both targets).
mod bounded_writer;
mod condense_input;
mod mesh_frame;
mod to_radio_frame;

pub(crate) use bounded_writer::*;
pub use condense_input::*;
pub use mesh_frame::*;
pub use to_radio_frame::*;

// `mesh-tokio`-only types — the Pi's host radio loop (std/alloc/tokio).
#[cfg(feature = "mesh-tokio")]
mod condensed;
#[cfg(feature = "mesh-tokio")]
mod mesh_config;
#[cfg(feature = "mesh-tokio")]
mod radio_error;
#[cfg(feature = "mesh-tokio")]
mod radio_handle;

#[cfg(feature = "mesh-tokio")]
pub use condensed::*;
#[cfg(feature = "mesh-tokio")]
pub use mesh_config::*;
#[cfg(feature = "mesh-tokio")]
pub use radio_error::*;
#[cfg(feature = "mesh-tokio")]
pub use radio_handle::*;

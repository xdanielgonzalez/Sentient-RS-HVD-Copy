//! Private prelude: re-exports the module's types plus the original external `use` lines, for
//! `impl_*.rs` / `type_*.rs` files to pull in with a single `use super::prelude::*;`.

pub use super::types::*;

pub use lib_io::host::{DirectionMode, SerialKind};
pub use lib_io::transport::{Parity, SerialParams, StopBits};
pub use lib_mqtt::broker::BrokerConfig;

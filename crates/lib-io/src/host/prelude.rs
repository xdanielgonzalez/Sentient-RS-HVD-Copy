//! Private prelude: re-exports the module's types plus the original external `use` lines
//! (paths bumped one level since this file lives one directory deeper than `host.rs` did).

pub use super::types::*;

pub use crate::transport::{Direction, IoError, Parity, SerialParams, SerialTransport, StopBits};
pub use std::os::unix::io::AsRawFd;
pub use std::time::Duration;
pub use tokio::io::{AsyncReadExt, AsyncWriteExt};
pub use tokio::time::{sleep, timeout};
pub use tokio_serial::{ClearBuffer, SerialPort, SerialPortBuilderExt};

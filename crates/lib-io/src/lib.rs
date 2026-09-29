//! `lib-io` — serial transport (RS-232 + RS-485) behind one async trait, **generic over the
//! reader** (`<R: SerialTransport>`, static dispatch, no `Box<dyn>`).
//!
//! * [`transport`] — the [`transport::SerialTransport`] trait + line params + RS-485 direction.
//! * `host` (feature `host`) — RPi Linux impls over `tokio-serial`, with an enum factory
//!   (`HostSerial`) that selects RS-232/RS-485 from config without boxing.
//! * `embassy_uart` (feature `embassy`) — reading via **embassy tasks + an injected async delay**;
//!   the MCU forward path. Embassy-version-free (see that module's doc comment): the concrete HAL
//!   UART/GPIO/delay types are supplied by the board crate, so this crate never pins an
//!   `embassy-*` version itself.
//! * [`driver`] — a generic poll driver that runs the Seplos exchange over any `SerialTransport`.
//! * `persist` (feature `host`) — disk / JSON-lines telemetry logging (Scope A): a bounded async
//!   writer that tails every published TMS payload (plus, separately, raw pre-parse wire captures)
//!   to a rotating jsonl log. std-only; folded from the former standalone `lib-persist` crate (#285).
//! * `ina3221` (feature `ina3221`) — an async driver for the TI INA3221 triple-channel I2C
//!   power/current monitor measuring the **adapter board's own rail draw** (diagnostics/UI only —
//!   NOT the project's power source, which is the Seplos BMS). Embassy-version-free like
//!   `embassy_uart` above; see that module's own doc comment for why it does not implement
//!   `spec_app::RailMonitor` directly (it would close a dependency cycle: `spec-app` already
//!   depends on this crate).
//!
//! # Example — the seam in miniature
//!
//! App logic programs against [`SerialTransport`]; the platform (host `tokio-serial`, embassy
//! UART) is just one impl of it. That is what makes the poll driver `cargo test`-able on the
//! host — a loopback stands in for the hardware:
//!
//! ```rust
//! use lib_io::{Direction, IoError, SerialTransport};
//!
//! /// In-memory loopback: bytes written come straight back on read — no hardware, no tty.
//! struct Loopback(Vec<u8>);
//!
//! impl SerialTransport for Loopback {
//!     async fn read(&mut self, buf: &mut [u8]) -> Result<usize, IoError> {
//!         let n = self.0.len().min(buf.len());
//!         buf[..n].copy_from_slice(&self.0[..n]);
//!         self.0.drain(..n);
//!         Ok(n)
//!     }
//!     async fn read_until(&mut self, delimiter: u8, buf: &mut [u8]) -> Result<usize, IoError> {
//!         // Frames are `\r`-terminated; deliver up to and including the delimiter.
//!         let end = match self.0.iter().position(|&b| b == delimiter) {
//!             Some(i) => i + 1,
//!             None => return Err(IoError::Timeout), // silent bus
//!         };
//!         if end > buf.len() {
//!             return Err(IoError::ShortTransfer);
//!         }
//!         buf[..end].copy_from_slice(&self.0[..end]);
//!         self.0.drain(..end);
//!         Ok(end)
//!     }
//!     async fn write_all(&mut self, bytes: &[u8]) -> Result<(), IoError> {
//!         self.0.extend_from_slice(bytes);
//!         Ok(())
//!     }
//!     async fn set_direction(&mut self, _dir: Direction) -> Result<(), IoError> {
//!         Ok(()) // full-duplex loopback: a no-op, exactly like RS-232
//!     }
//! }
//!
//! # fn main() -> Result<(), IoError> {
//! # let rt = tokio::runtime::Builder::new_current_thread()
//! #     .build()
//! #     .map_err(|_| IoError::Device)?;
//! # rt.block_on(async {
//! let mut port = Loopback(Vec::new());
//! port.write_all(b"~TEST-FRAME\r").await?;
//!
//! let mut buf = [0u8; 32];
//! let n = port.read_until(b'\r', &mut buf).await?;
//! assert_eq!(&buf[..n], b"~TEST-FRAME\r");
//! # Ok(())
//! # })
//! # }
//! ```
#![cfg_attr(not(feature = "host"), no_std)]
// `deny` (not `forbid`): the host-only RS-485 direction-control seam needs exactly one unsafe FFI
// call site — the kernel `TIOCSRS485` ioctl plus a `tcdrain` drain call (`host::rs485_ioctl`,
// `#![allow(unsafe_code)]` + SAFETY comments on every `unsafe` block) — for which no safe wrapper
// exists anywhere in this workspace's dependency graph (CLAUDE.md's *Abstraction & Platform
// Seams*: "keep the FFI/C shim DUMB"). `forbid` cannot be locally overridden by an inner
// `#[allow]` at all; `deny` can be, so this is the minimal relaxation — every other module in this
// crate (including `host::gpio_de`, which is 100% safe Rust over the `gpiocdev` crate) is still
// unsafe-code-clean, and a stray `unsafe` anywhere but that one module still fails the build.
#![deny(unsafe_code)]

pub mod driver;
pub mod transport;

#[cfg(any(test, feature = "mock"))]
pub mod mock;

#[cfg(feature = "host")]
pub mod host;

// Disk / JSON-lines telemetry logging (Scope A) — folded from the former standalone `lib-persist`
// crate (#285). std-only (tokio/serde_json): gated behind `host` so it never enters the `no_std`
// MCU build of this crate (see `persist`'s own module docs).
#[cfg(feature = "host")]
pub mod persist;

#[cfg(feature = "embassy")]
pub mod embassy_uart;

#[cfg(feature = "ina3221")]
pub mod ina3221;

pub use transport::{Direction, IoError, Parity, SerialParams, SerialTransport, StopBits};

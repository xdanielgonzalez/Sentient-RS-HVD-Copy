//! Embassy MCU serial transport — reading via **embassy tasks**, gated by the `embassy` feature.
//! The forward path (built for the target via the pipeline / `west`/`cargo` for the board crate,
//! never as part of a host `cargo build` of THIS crate).
//!
//! **Embassy-version-free by construction.** This module names no `embassy-*` crate at all — the
//! UART (`U`), driver-enable GPIO (`D`), and RS-485 guard-delay (`T`) are generic over the
//! `embedded-io-async`/`embedded-hal`/`embedded-hal-async` traits only. The concrete embassy HAL
//! types (and whichever `embassy-time`/`embassy-executor` version the board crate pins) are
//! supplied entirely by the caller. This matters because the workspace pins TWO different embassy
//! generations across the graph (main workspace `embassy-time` 0.4 for `lib-net`'s embassy-net
//! spike; the excluded `bin/mcu-rp2350` workspace pins `embassy-rp` 0.10 / `embassy-time` 0.5) —
//! `embassy-time-driver`'s `links = "embassy-time"` key allows only ONE version per graph, so this
//! crate (a dependency of both) must not pin either. `bin/mcu-rp2350` supplies its own
//! `embassy_time::Delay` (or equivalent) as `T: embedded_hal_async::delay::DelayNs` at the call
//! site instead.
//!
//! One `SerialTransport` impl over an async UART, plus a reusable generic reader loop that
//! drives the [`crate::driver`] and forwards `CanonicalReading`s over a channel. RS-485 direction
//! toggles a HAL `Output` GPIO (driver-enable) with pre/post-TX guard delays via the injected `T`.

use crate::transport::{Direction, IoError, SerialTransport};
use embedded_hal::digital::OutputPin;
use embedded_hal_async::delay::DelayNs;
use embedded_io_async::{Read, Write};

/// An embassy-shaped async UART transport. Generic over the platform's `embedded_io_async` UART
/// type `U`, the driver-enable GPIO `D`, and an injected async delay `T` — no `Box<dyn>`; the
/// board crate supplies concrete HAL types (and picks its own embassy-time generation for `T`).
///
/// Implements [`SerialTransport`] where
/// `U: embedded_io_async::Read + embedded_io_async::Write`,
/// `D: embedded_hal::digital::OutputPin`, and `T: embedded_hal_async::delay::DelayNs`:
/// - `read`/`write_all` delegate to the `embedded_io_async` UART, mapping errors to `IoError`.
/// - `read_until` reads bytes until the delimiter or `buf` fills (`ShortTransfer`).
/// - `set_direction` drives the DE pin high for `Transmit` / low for `Receive`, with `delay`
///   guard delays (5 ms pre-TX settle, 10 ms post-TX before RX).
pub struct EmbassyUart<U, D, T> {
    pub uart: U,
    pub dir_enable: D,
    /// Injected async delay (e.g. the board crate's `embassy_time::Delay`) — keeps this crate
    /// embassy-version-free; see this module's doc comment.
    pub delay: T,
}

impl<U, D, T> SerialTransport for EmbassyUart<U, D, T>
where
    U: Read + Write,
    D: OutputPin,
    T: DelayNs,
{
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, IoError> {
        self.uart.read(buf).await.map_err(|_| IoError::Device)
    }

    async fn write_all(&mut self, bytes: &[u8]) -> Result<(), IoError> {
        self.uart
            .write_all(bytes)
            .await
            .map_err(|_| IoError::Device)
    }

    async fn read_until(&mut self, delim: u8, buf: &mut [u8]) -> Result<usize, IoError> {
        let mut count = 0;
        let mut tmp = [0u8; 1];

        for slot in buf.iter_mut() {
            self.uart
                .read_exact(&mut tmp)
                .await
                .map_err(|_| IoError::Device)?;
            *slot = tmp[0];
            count += 1;

            if tmp[0] == delim {
                return Ok(count);
            }
        }

        // Buffer filled without finding delimiter
        Err(IoError::ShortTransfer)
    }

    async fn set_direction(&mut self, dir: Direction) -> Result<(), IoError> {
        match dir {
            Direction::Transmit => {
                // Enable transmitter, then settle the line
                self.dir_enable.set_high().map_err(|_| IoError::Direction)?;
                self.delay.delay_ms(5).await;
            }
            Direction::Receive => {
                // Post-TX guard delay, then switch to RX
                self.delay.delay_ms(10).await;
                self.dir_enable.set_low().map_err(|_| IoError::Direction)?;
            }
        }
        Ok(())
    }
}

/// Generic inner loop for an embassy reader task.
///
/// This function encapsulates the core polling loop that an `#[embassy_executor::task]`
/// in the board crate wraps. Because embassy tasks require concrete (non-generic) types,
/// the task itself must be monomorphized per platform; this reusable inner `async fn`
/// provides the generic behavior.
///
/// The task wrapper in the board crate will typically look like:
/// ```ignore
/// #[embassy_executor::task]
/// async fn poll_uart_task(
///     uart: EmbassyUart<ConcreteUart, ConcreteGpio, ConcreteDelay>,
///     channel: Sender<'static, CanonicalReading, 16>,
/// ) {
///     poll_forever(uart, Seplos::new(), 0x2B).await
/// }
/// ```
///
/// # Arguments
/// - `uart` — the configured `EmbassyUart<U, D, T>` transport (its own `delay: T` also paces the
///   100 ms inter-exchange gap below, so no separate delay type is needed here).
/// - `vendor` — implements `BmsVendorProtocol` for the target vendor (e.g. Seplos).
/// - `address` — the BMS device address (slave ID).
/// - `channel` — an `embassy_sync::channel::Sender` on which to publish readings.
///   (Currently unimplemented; a board-crate task will complete this.)
///
/// This function never returns (infinite loop, returns `!`).
pub async fn poll_forever<U, D, T, V>(mut uart: EmbassyUart<U, D, T>, vendor: V, address: u8) -> !
where
    U: Read + Write,
    D: OutputPin,
    T: DelayNs,
    V: lib_parse::vendor::BmsVendorProtocol,
{
    // Allocate a fixed-size buffer for telemetry exchanges.
    // Seplos frames are ~50–80 bytes; use 256 for headroom.
    let mut buf = [0u8; 256];

    loop {
        // Read one telemetry sample from the BMS at the given address.
        match crate::driver::read_telemetry(&mut uart, &vendor, address, &mut buf).await {
            Ok(_reading) => {
                // TODO: board crate forwards reading on an embassy_sync channel here.
                // For now, drop the reading silently (telemetry exchange succeeded).
            }
            Err(_e) => {
                // TODO: log or track exchange failures.
                // For now, silently retry on the next loop iteration.
            }
        }

        // Add a small delay between exchanges to avoid hammering the BMS.
        uart.delay.delay_ms(100).await;
    }
}

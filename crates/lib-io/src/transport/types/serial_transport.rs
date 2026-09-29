use super::direction::Direction;
use super::io_error::IoError;

/// A byte transport for the Seplos exchange. Async so an embassy MCU UART and a host async serial
/// port share one interface. Implementors are used generically — never boxed.
///
/// The `async fn`-in-trait form keeps it usable in both `no_std` embassy and host tokio contexts.
///
/// # Example — callers stay generic
///
/// The half-duplex exchange shape (`Transmit` → write → `Receive` → read-until-`\r`) is written
/// once against the trait and monomorphizes onto any impl — a real port or a scripted stub:
///
/// ```rust
/// use lib_io::{Direction, IoError, SerialTransport};
///
/// /// One request/response over ANY transport — host tokio-serial, embassy UART, or a stub.
/// async fn exchange<T: SerialTransport>(
///     port: &mut T,
///     request: &[u8],
///     buf: &mut [u8],
/// ) -> Result<usize, IoError> {
///     port.set_direction(Direction::Transmit).await?;
///     port.write_all(request).await?;
///     port.set_direction(Direction::Receive).await?;
///     port.read_until(b'\r', buf).await
/// }
///
/// # struct OneReply(&'static [u8]);
/// # impl SerialTransport for OneReply {
/// #     async fn read(&mut self, buf: &mut [u8]) -> Result<usize, IoError> {
/// #         self.read_until(b'\r', buf).await
/// #     }
/// #     async fn read_until(&mut self, _delim: u8, buf: &mut [u8]) -> Result<usize, IoError> {
/// #         let n = self.0.len();
/// #         if n > buf.len() {
/// #             return Err(IoError::ShortTransfer);
/// #         }
/// #         buf[..n].copy_from_slice(self.0);
/// #         Ok(n)
/// #     }
/// #     async fn write_all(&mut self, _bytes: &[u8]) -> Result<(), IoError> {
/// #         Ok(())
/// #     }
/// #     async fn set_direction(&mut self, _dir: Direction) -> Result<(), IoError> {
/// #         Ok(())
/// #     }
/// # }
/// # fn main() -> Result<(), IoError> {
/// # let rt = tokio::runtime::Builder::new_current_thread()
/// #     .build()
/// #     .map_err(|_| IoError::Device)?;
/// # rt.block_on(async {
/// let mut port = OneReply(b"~REPLY\r"); // stand-in for an opened `HostSerial`
/// let mut buf = [0u8; 64];
/// let n = exchange(&mut port, b"~REQ\r", &mut buf).await?;
/// assert_eq!(&buf[..n], b"~REPLY\r");
/// # Ok(())
/// # })
/// # }
/// ```
#[allow(async_fn_in_trait)]
pub trait SerialTransport {
    /// Read into `buf`, returning the number of bytes read (may be short).
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, IoError>;

    /// Read until `delimiter` (inclusive) or `buf` fills, returning bytes written. Used for the
    /// Seplos `\r`-terminated frames. Returns `IoError::ShortTransfer` if `buf` fills first.
    async fn read_until(&mut self, delimiter: u8, buf: &mut [u8]) -> Result<usize, IoError>;

    /// Write all of `bytes`.
    async fn write_all(&mut self, bytes: &[u8]) -> Result<(), IoError>;

    /// Set the RS-485 bus direction (no-op on RS-232). Called around a transmit for half-duplex.
    async fn set_direction(&mut self, dir: Direction) -> Result<(), IoError>;

    /// Drop any buffered/unread RX bytes still sitting in the transport's input buffer.
    ///
    /// Called on the exchange error/timeout path (see [`crate::driver`]'s `read_telemetry`,
    /// `read_fault`, and the Voltronic continuation-read loop), before the next request goes
    /// out: a failed or timed-out exchange can leave a stale partial frame buffered underneath,
    /// which would otherwise desync the NEXT `read_until` (its first bytes would be leftovers
    /// from the previous exchange, not the new response).
    ///
    /// Default is a no-op — correct for any transport with no separate RX buffer to clear (an
    /// in-memory loopback/mock, or an embassy UART with no discrete input FIFO exposed through
    /// `embedded_io_async`). Real host transports (backed by `tokio-serial`/`serialport`) override
    /// this to actually clear the OS-level input buffer.
    async fn discard_input(&mut self) -> Result<(), IoError> {
        Ok(())
    }
}

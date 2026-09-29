use lib_io::host::HostSerial;

use super::capture_handle::CaptureHandle;

/// [`SerialTransport`](lib_io::transport::SerialTransport) DECORATOR that mirrors every
/// request/response exchange to the raw pre-parse wire-capture writer (decision 0001's open risk —
/// "this environment had no real Voltronic/QPIGS wire captures... must be verified against real
/// captures... before an inverter deployment is trusted"; see `lib_io::persist::RawCaptureRecord`'s
/// doc comment), while delegating every actual I/O call to `inner` unchanged. See
/// `impl_capture_serial.rs` for the `SerialTransport` impl, the write/read pairing contract, and the
/// `handle.is_none()` fast-path guarantee (a pure passthrough — no allocation, no copy, beyond one
/// branch — when `[capture]` isn't configured).
///
/// `HostPlatform::serial` is this type (not a bare `HostSerial`) unconditionally: whether capturing
/// actually happens is entirely a runtime property of `handle` (`None` vs `Some`), not a
/// compile-time one — see `HostPlatform::build_port`/`reconnect`.
pub struct CaptureSerial {
    /// The real transport every call ultimately delegates to.
    pub(crate) inner: HostSerial,
    /// `None` when `[capture]` isn't configured for this bus — the fast-path passthrough case.
    /// `Some` clones cheaply (an `mpsc::Sender` + an `Arc<AtomicU64>` + a `String`) into every
    /// reopened `CaptureSerial` on `HostPlatform::reconnect`, so a reconnected port keeps recording
    /// to the SAME writer/channel.
    pub(crate) handle: Option<CaptureHandle>,
    /// Bytes staged by the most recent [`lib_io::transport::SerialTransport::write_all`] call, held until the NEXT
    /// successful [`lib_io::transport::SerialTransport::read_until`] pairs them into one `RawCaptureRecord` — exchanges
    /// are strictly sequential per bus (one `write_all` fully completes before the `read_until` that
    /// answers it), so pairing the LAST staged write with the NEXT successful read is the correct
    /// exchange. Left empty whenever `handle` is `None` (the fast-path guard in `write_all` skips
    /// the push entirely) or once a `read_until` drains it.
    pub(crate) pending_tx: Vec<u8>,
}

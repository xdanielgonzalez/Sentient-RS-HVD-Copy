use lib_mapping::schema::DeviceRailReading;

/// Physical channel count of the INA3221 triple-channel monitor. Bounds the `out` slice a caller
/// of [`RailMonitor::read_all`] typically supplies — two rails wired (`Ethernet`, `Radio`) plus
/// one spare channel.
pub const MAX_DEVICE_RAILS: usize = 3;

/// A multi-channel **device-rail** monitor: the seam the INA3221 driver sits behind. This
/// measures the adapter board's OWN rail draw (W5500, Heltec) — engineering self-telemetry for
/// diagnostics + the on-device UI, NOT the project's power source (that is the Seplos BMS, read
/// via [`crate::Platform::emit_reading`]/mapped in `lib_mapping::transform`). Readings from this
/// seam are published on a non-TMS diagnostic topic only — see [`crate::Platform::emit_rail_draw`].
///
/// Async so an embassy I2C bus and a host mock share one interface, mirroring
/// [`lib_io::transport::SerialTransport`]'s async-trait shape for the same reason. Implementors are
/// used generically — never boxed.
///
/// # Example — callers stay generic
///
/// The read shape (fill a fixed buffer, get back a count) is written once against the trait and
/// monomorphizes onto any impl — a real INA3221 or a scripted stub:
///
/// ```rust
/// use lib_mapping::schema::{DeviceRail, DeviceRailReading};
/// use spec_app::{RailMonitor, MAX_DEVICE_RAILS};
///
/// # fn block_on<F: core::future::Future>(fut: F) -> F::Output {
/// #     let mut fut = core::pin::pin!(fut);
/// #     let mut cx = core::task::Context::from_waker(core::task::Waker::noop());
/// #     loop {
/// #         if let core::task::Poll::Ready(out) = fut.as_mut().poll(&mut cx) {
/// #             return out;
/// #         }
/// #     }
/// # }
/// /// One read cycle over ANY monitor.
/// async fn poll<M: RailMonitor>(monitor: &mut M) -> [DeviceRailReading; MAX_DEVICE_RAILS] {
///     let blank = DeviceRailReading { rail: DeviceRail::Spare, shunt_uv: 0, bus_mv: 0 };
///     let mut out = [blank; MAX_DEVICE_RAILS];
///     let n = monitor.read_all(&mut out).await;
///     assert_eq!(n, 2); // only Ethernet + Radio are wired on the bench
///     out
/// }
///
/// # struct TwoRails;
/// # impl RailMonitor for TwoRails {
/// #     async fn read_all(&mut self, out: &mut [DeviceRailReading]) -> usize {
/// #         let readings = [
/// #             DeviceRailReading { rail: DeviceRail::Ethernet, shunt_uv: 7_500, bus_mv: 5_050 },
/// #             DeviceRailReading { rail: DeviceRail::Radio, shunt_uv: 4_000, bus_mv: 3_300 },
/// #         ];
/// #         let n = readings.len().min(out.len());
/// #         out[..n].copy_from_slice(&readings[..n]);
/// #         n
/// #     }
/// # }
/// let mut monitor = TwoRails;
/// let readings = block_on(poll(&mut monitor));
/// assert_eq!(readings[0].rail, DeviceRail::Ethernet);
/// assert_eq!(readings[0].bus_mv, 5_050);
/// ```
#[allow(async_fn_in_trait)]
pub trait RailMonitor {
    /// Read every wired channel in one I2C exchange, writing into `out` and returning how many
    /// entries were written (`<= out.len()`, and `<= MAX_DEVICE_RAILS` — the INA3221's physical
    /// channel count). A channel with no rail wired (`Spare` on the bench today) is simply not
    /// written — the caller sizes `out` to `MAX_DEVICE_RAILS` and uses only the first `n` entries.
    async fn read_all(&mut self, out: &mut [DeviceRailReading]) -> usize;
}

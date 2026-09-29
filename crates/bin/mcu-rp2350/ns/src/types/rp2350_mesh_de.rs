/// No-op RS-485 driver-enable stand-in for [`lib_io::embassy_uart::EmbassyUart`]'s `D` generic on
/// the Heltec mesh UART leg (WP3, delight-features). This leg is a plain full-duplex UART straight
/// into the Heltec's Serial Module (no direction pin at all — matches `lib_io::host`'s RS-232 impls'
/// `set_direction` no-op posture), but `EmbassyUart<U, D, T>`'s `SerialTransport` impl still requires
/// `D: embedded_hal::digital::OutputPin` regardless of whether `set_direction` is ever actually
/// called on this leg — this zero-sized type satisfies that bound trivially (see
/// `impl_rp2350_mesh_de.rs`) so the mesh leg never has to sacrifice a spare GPIO to a pin nothing
/// drives.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct Rp2350MeshDe;

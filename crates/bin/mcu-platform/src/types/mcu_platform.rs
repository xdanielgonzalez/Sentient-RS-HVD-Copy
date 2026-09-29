use super::mcu_clock::McuClock;
use lib_io::embassy_uart::EmbassyUart;
use lib_mapping::sink::Publisher;

/// The Embassy MCU platform: an [`EmbassyUart`] transport + a [`Publisher`] sink + an injected
/// [`McuClock`]. Embassy-version-free by construction (see the crate root doc comment) — `U`/`D`/`T`
/// (the UART/GPIO/delay generics `EmbassyUart` itself takes), `P: Publisher`, and `C: McuClock` are
/// all supplied by the board crate (`bin/mcu-rp2350`), which alone picks concrete embassy types.
pub struct McuPlatform<U, D, T, P: Publisher, C: McuClock> {
    pub transport: EmbassyUart<U, D, T>,
    pub publisher: P,
    pub clock: C,
}

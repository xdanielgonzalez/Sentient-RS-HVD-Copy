use embassy_rp::peripherals::PIO0;
use embassy_rp::pio_programs::ws2812::{Grb, PioWs2812};

/// Concrete monomorphization of embassy-rp 0.10's generic PIO WS2812 driver
/// (`embassy_rp::pio_programs::ws2812::PioWs2812`) for THIS board's onboard NeoPixel status LED
/// (feature `neopixel`, default OFF, HIL debugging aid — see `boot_neopixel.rs`'s module doc):
/// PIO0 / state machine 0 (both otherwise entirely unused in this crate), exactly one LED
/// (`N = 1` — the Feather has a single onboard WS2812, not a strip), default GRB byte order (the
/// common default for WS2812B, per that module's own doc comment — this board's onboard pixel is a
/// WS2812B). Mirrors `Rp2350Display`'s own "one alias per concrete instantiation, under `types/`"
/// convention.
pub(crate) type Rp2350NeoPixel = PioWs2812<'static, PIO0, 0, 1, Grb>;

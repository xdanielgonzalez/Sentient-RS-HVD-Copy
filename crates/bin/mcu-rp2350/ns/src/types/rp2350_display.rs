use ssd1306::mode::BufferedGraphicsModeAsync;
use ssd1306::prelude::{DisplaySize128x32, I2CInterface};
use ssd1306::Ssd1306Async;

use super::rp2350_i2c_bus::Rp2350I2c1Bus;

/// The board's concrete SSD1306 driver instantiation (WP5) — Adafruit PID 2900 (128x32), async
/// buffered-graphics mode directly over this crate's [`Rp2350I2c1Bus`] (I2C1, GP2/GP3) — singly
/// owned, no shared-bus device wrapper (see `boot_i2c.rs`'s module doc for why the earlier
/// shared-bus arrangement was removed). `Ssd1306Async<DI, SIZE, BufferedGraphicsModeAsync<SIZE>>`
/// implements `embedded_graphics::draw_target::DrawTarget<Color = BinaryColor>` (verified against
/// ssd1306 0.10.0's own docs — see `Cargo.toml`'s comment history), which is exactly the seam
/// `ns::ui::ScreenView::render` is generic over — so a `&mut Rp2350Display` satisfies it directly,
/// with no adapter needed. Constructed once in `boot_ui.rs::build_display`, then owned entirely by
/// `boot_ui::display_task` — nothing else touches it.
#[cfg(feature = "ui")]
pub(crate) type Rp2350Display = Ssd1306Async<
    I2CInterface<Rp2350I2c1Bus>,
    DisplaySize128x32,
    BufferedGraphicsModeAsync<DisplaySize128x32>,
>;

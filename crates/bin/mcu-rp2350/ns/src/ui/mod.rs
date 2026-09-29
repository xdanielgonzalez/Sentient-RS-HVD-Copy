//! OLED FeatherWing UI — pure render layer only. Four screens
//! (`Screen::{Network, Meshtastic, Battery, About}`) drawn against a 128x32 SSD1306 (Adafruit PID
//! 2900) via the `embedded_graphics::draw_target::DrawTarget<Color = BinaryColor>` seam. No
//! `ssd1306` driver, I2C, or GPIO here; gated behind this crate's `ui` feature (off by default) and
//! not yet wired into `main.rs`. [`types::Screen`]/[`types::ScreenView`] dispatch statically (no
//! `dyn`, no alloc) over [`types::UiSnapshot`], the frozen data shape each screen renders from;
//! [`types::ButtonEvent`] only names the event shape a later wiring wave produces. No widget
//! toolkit: `embedded-gui`/Slint/LVGL are rejected (no global allocator on this target) — four
//! lines across four screens doesn't need one.

pub mod types;

mod impl_about_screen;
mod impl_battery_screen;
mod impl_fmt;
mod impl_meshtastic_screen;
mod impl_network_screen;
mod impl_screen;
mod impl_ui_snapshot;

pub use types::{Button, ButtonEvent, DhcpState, Screen, ScreenView, UiSnapshot};

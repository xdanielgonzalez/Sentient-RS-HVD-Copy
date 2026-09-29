//! [`Screen`] — which of the four OLED screens is on-glass — and the [`ScreenView`] rendering
//! contract every screen implements.

use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::pixelcolor::BinaryColor;

use super::ui_snapshot::UiSnapshot;

/// Which of the four 128x32 OLED screens is currently displayed. Cycles in a fixed ring via
/// [`Screen::next`]/[`Screen::prev`] (impls live in `impl_screen.rs`, module root, per the repo's
/// type/impl split) — the wiring layer (a later wave) calls these from debounced button-C/button-A
/// edges respectively; see [`super::button_event::ButtonEvent`]'s doc for the suggested mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    /// IP/DHCP/broker status.
    Network,
    /// Meshtastic node number + last-send status.
    Meshtastic,
    /// Battery SOC/voltage/current — Seplos BMS-sourced, the project's actual power telemetry.
    Battery,
    /// Firmware version + uptime + the two INA3221 adapter-rail-draw diagnostics (Ethernet,
    /// Radio) — the device/diagnostics screen. Deliberately NOT on `Battery`: see
    /// `ui_snapshot.rs`'s module doc for why the rail figures live here instead.
    About,
}

/// One screen's rendering contract, implemented directly on [`Screen`] (`impl_screen.rs`) via a
/// single `match` that dispatches into four `#[inline(never)]` free functions — one per
/// `impl_*_screen.rs` — NOT via `dyn ScreenView` trait objects (this target has no allocator) and
/// NOT via four separate marker types (a plain `match` on the enum itself is simpler and gives the
/// exact same static dispatch).
///
/// # Cortex-M frame hazard
///
/// Rust reserves the **max stack frame across all `match` arms** in a function, so if the four
/// screens' render bodies lived directly in `Screen::render`'s own function, the cheapest screen
/// would pay for the most expensive screen's locals on every call. Splitting each arm's body into
/// its own `#[inline(never)]` fn (see each `impl_*_screen.rs`) is the fix — the same lesson that
/// took `emit_reading`'s frame from 1712 B to 16 B on this same MCU target (`crates/lib-mapping`).
pub trait ScreenView {
    /// Draw this screen's content into `d`, reading from `snap`. Terse by necessity: the physical
    /// target is a 128x32 SSD1306 (Adafruit PID 2900) — roughly four lines of 6x10 text, not a
    /// 128x64 canvas.
    fn render<D: DrawTarget<Color = BinaryColor>>(
        &self,
        d: &mut D,
        snap: &UiSnapshot,
    ) -> Result<(), D::Error>;
}

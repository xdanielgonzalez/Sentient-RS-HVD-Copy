//! `Screen::next`/`Screen::prev` (the fixed four-screen ring) and `impl ScreenView for Screen` (the
//! single dispatch `match`). See `types/screen.rs`'s doc for why `ScreenView` is implemented
//! directly on the enum rather than via four marker types or `dyn` trait objects, and for the
//! Cortex-M frame hazard this dispatch match is deliberately shaped around.

use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::pixelcolor::BinaryColor;

use super::impl_about_screen::render_about_screen;
use super::impl_battery_screen::render_battery_screen;
use super::impl_meshtastic_screen::render_meshtastic_screen;
use super::impl_network_screen::render_network_screen;
use super::types::{Screen, ScreenView, UiSnapshot};

impl Screen {
    /// Advance to the next screen in the fixed ring. Suggested wiring: button C.
    pub fn next(self) -> Self {
        match self {
            Screen::Network => Screen::Meshtastic,
            Screen::Meshtastic => Screen::Battery,
            Screen::Battery => Screen::About,
            Screen::About => Screen::Network,
        }
    }

    /// Step back to the previous screen in the fixed ring. Suggested wiring: button A.
    pub fn prev(self) -> Self {
        match self {
            Screen::Network => Screen::About,
            Screen::Meshtastic => Screen::Network,
            Screen::Battery => Screen::Meshtastic,
            Screen::About => Screen::Battery,
        }
    }
}

impl ScreenView for Screen {
    /// The ONE dispatch match this module's whole file exists to protect: every arm calls straight
    /// into a `#[inline(never)]` free function in its own `impl_*_screen.rs`, so THIS fn's stack
    /// frame never has to reserve room for the largest screen's locals — only its own (a `match` on
    /// a 4-variant enum plus a tail call).
    fn render<D: DrawTarget<Color = BinaryColor>>(
        &self,
        d: &mut D,
        snap: &UiSnapshot,
    ) -> Result<(), D::Error> {
        match self {
            Screen::Network => render_network_screen(d, snap),
            Screen::Meshtastic => render_meshtastic_screen(d, snap),
            Screen::Battery => render_battery_screen(d, snap),
            Screen::About => render_about_screen(d, snap),
        }
    }
}

#[cfg(test)]
mod tests {
    use embedded_graphics::mock_display::MockDisplay;

    use super::*;

    #[test]
    fn next_cycles_through_all_four_and_back_to_network() {
        let s = Screen::Network;
        assert_eq!(s.next(), Screen::Meshtastic);
        assert_eq!(s.next().next(), Screen::Battery);
        assert_eq!(s.next().next().next(), Screen::About);
        assert_eq!(s.next().next().next().next(), Screen::Network);
    }

    #[test]
    fn prev_cycles_backward_through_all_four() {
        let s = Screen::Network;
        assert_eq!(s.prev(), Screen::About);
        assert_eq!(s.prev().prev(), Screen::Battery);
        assert_eq!(s.prev().prev().prev(), Screen::Meshtastic);
        assert_eq!(s.prev().prev().prev().prev(), Screen::Network);
    }

    #[test]
    fn next_and_prev_are_inverses() {
        for s in [
            Screen::Network,
            Screen::Meshtastic,
            Screen::Battery,
            Screen::About,
        ] {
            assert_eq!(s.next().prev(), s);
            assert_eq!(s.prev().next(), s);
        }
    }

    #[test]
    fn render_dispatches_every_variant_without_error() {
        // Smoke test only: each screen's own golden pixel test lives in its own
        // `impl_*_screen.rs`. This just proves `Screen::render`'s match wires every arm to SOME
        // renderer (a copy-paste slip that routed two arms to the same fn would still show up
        // there, not here).
        let snap = UiSnapshot::default();
        for screen in [
            Screen::Network,
            Screen::Meshtastic,
            Screen::Battery,
            Screen::About,
        ] {
            let mut d: MockDisplay<BinaryColor> = MockDisplay::new();
            d.set_allow_out_of_bounds_drawing(true);
            screen.render(&mut d, &snap).unwrap();
        }
    }
}

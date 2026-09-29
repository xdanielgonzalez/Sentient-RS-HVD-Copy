//! Render body for [`Screen::About`](super::types::Screen::About) — firmware version + uptime,
//! plus the INA3221 adapter-rail-draw diagnostics (Ethernet/Radio). This is the device/diagnostics
//! screen: the rail figures render HERE, under an explicit `DEV` line, rather than on Battery —
//! they are the adapter board's OWN power draw, not the Seplos-sourced pack SOC/voltage/current
//! Battery shows (`impl_battery_screen.rs`). See `ui_snapshot.rs`'s module doc for the correction
//! this split records, and `impl_network_screen.rs`'s module doc for why this render body lives in
//! its own `#[inline(never)]` fn (Cortex-M frame hazard).

use core::fmt::Write as _;

use embedded_graphics::mono_font::ascii::FONT_6X10;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;
use embedded_graphics::text::{Baseline, Text};
use heapless::String;

use super::types::UiSnapshot;

/// Same 8 px line pitch as `impl_network_screen.rs` — see that module's `LINE_Y` doc for the
/// FONT_6X10-into-32px trade-off this repeats.
const LINE_Y: [i32; 4] = [0, 8, 16, 24];

#[inline(never)]
pub(super) fn render_about_screen<D: DrawTarget<Color = BinaryColor>>(
    d: &mut D,
    snap: &UiSnapshot,
) -> Result<(), D::Error> {
    let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);

    Text::with_baseline("ABOUT", Point::new(0, LINE_Y[0]), style, Baseline::Top).draw(d)?;

    let mut fw_line: String<24> = String::new();
    let _ = write!(fw_line, "FW {}", snap.fw_version);
    Text::with_baseline(&fw_line, Point::new(0, LINE_Y[1]), style, Baseline::Top).draw(d)?;

    // Deliberately raw seconds, not H:M:S — a later wave can add that formatting if the terse
    // display proves insufficient; this is the minimum honest value for a first cut.
    let mut uptime_line: String<24> = String::new();
    let _ = write!(uptime_line, "UP {}s", snap.uptime_secs);
    Text::with_baseline(&uptime_line, Point::new(0, LINE_Y[2]), style, Baseline::Top).draw(d)?;

    // Device-diagnostics line: the INA3221's own adapter-rail draw (Ethernet/Radio), explicitly
    // labeled `DEV` so it reads as device self-telemetry, never as battery/pack data — see this
    // file's module doc for the correction this records. Replaces the earlier static
    // "RP2350 FEATHER" line: the 128x32 display only has room for 4 lines of 6x10 text, and this
    // diagnostic is the more useful use of that 4th line.
    // WIDTH GUARD: the panel is 128 px and FONT_6X10 is 6 px/glyph, so exactly 21 glyphs fit.
    // "DEV ETH{}mA LORA{}mA" is 16 literal chars, leaving only 5 for BOTH rail values — two
    // 3-digit readings (entirely normal: the Heltec draws 3 digits under TX load) would render
    // 22 chars = 132 px and the trailing glyph would be silently dropped off the edge, not
    // wrapped. Found by the host ASCII preview harness; the MockDisplay golden tests could not
    // catch it because MockDisplay is capped at 64 px, so nothing past column 64 is ever
    // exercised. Clamp each value to 3 digits (999 mA is far above any rail this board drives)
    // so the worst case is exactly 21 glyphs = 126 px, inside the panel with 2 px to spare.
    let eth = snap.rail_eth_ma.clamp(0, 999);
    let radio = snap.rail_radio_ma.clamp(0, 999);
    let mut rail_line: String<24> = String::new();
    let _ = write!(rail_line, "DEV ETH{}mA LORA{}mA", eth, radio);
    Text::with_baseline(&rail_line, Point::new(0, LINE_Y[3]), style, Baseline::Top).draw(d)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use embedded_graphics::mock_display::MockDisplay;

    use super::*;

    /// See `impl_network_screen.rs::tests::expected_display`'s doc for what this oracle
    /// methodology proves and its documented 64 px `MockDisplay` window caveat.
    fn expected_display(lines: [&str; 4]) -> MockDisplay<BinaryColor> {
        let mut expected: MockDisplay<BinaryColor> = MockDisplay::new();
        expected.set_allow_out_of_bounds_drawing(true);
        let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
        for (line, y) in lines.iter().zip(LINE_Y) {
            Text::with_baseline(line, Point::new(0, y), style, Baseline::Top)
                .draw(&mut expected)
                .unwrap();
        }
        expected
    }

    #[test]
    fn renders_version_uptime_and_rail_draw() {
        let snap = UiSnapshot {
            fw_version: "0.1.0",
            uptime_secs: 3661,
            rail_eth_ma: 120,
            rail_radio_ma: 45,
            ..UiSnapshot::default()
        };

        let mut actual: MockDisplay<BinaryColor> = MockDisplay::new();
        actual.set_allow_out_of_bounds_drawing(true);
        render_about_screen(&mut actual, &snap).unwrap();

        actual.assert_eq(&expected_display([
            "ABOUT",
            "FW 0.1.0",
            "UP 3661s",
            "DEV ETH120mA LORA45mA",
        ]));
    }

    #[test]
    fn renders_default_snapshot() {
        let snap = UiSnapshot::default();

        let mut actual: MockDisplay<BinaryColor> = MockDisplay::new();
        actual.set_allow_out_of_bounds_drawing(true);
        render_about_screen(&mut actual, &snap).unwrap();

        actual.assert_eq(&expected_display([
            "ABOUT",
            "FW 0.0.0",
            "UP 0s",
            "DEV ETH0mA LORA0mA",
        ]));
    }
}

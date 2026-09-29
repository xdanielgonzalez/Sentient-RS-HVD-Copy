//! Render body for [`Screen::Battery`](super::types::Screen::Battery) — battery SOC/voltage/current,
//! sourced ENTIRELY from the Seplos BMS (the project's actual power telemetry). This screen
//! deliberately shows nothing else: the INA3221 adapter-rail-draw figures (Ethernet/Radio) are NOT
//! battery data and render on the About/device screen instead (`impl_about_screen.rs`) — see that
//! correction's rationale in `ui_snapshot.rs`'s module doc. See `impl_network_screen.rs`'s module
//! doc for why this render body lives in its own `#[inline(never)]` fn (Cortex-M frame hazard).

use embedded_graphics::mono_font::ascii::FONT_6X10;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;
use embedded_graphics::text::{Baseline, Text};
use heapless::String;

use super::impl_fmt::{fmt_centi_1dp, fmt_centi_1dp_signed, fmt_deci_1dp};
use super::types::UiSnapshot;

/// Same 8 px line pitch as `impl_network_screen.rs` — see that module's `LINE_Y` doc for the
/// FONT_6X10-into-32px trade-off this repeats. Only the first 3 slots are used: this screen has no
/// 4th line of content (the INA3221 rail-draw line that used to occupy it moved to the About
/// screen — see this file's module doc).
const LINE_Y: [i32; 3] = [0, 8, 16];

#[inline(never)]
pub(super) fn render_battery_screen<D: DrawTarget<Color = BinaryColor>>(
    d: &mut D,
    snap: &UiSnapshot,
) -> Result<(), D::Error> {
    let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);

    Text::with_baseline("BATTERY", Point::new(0, LINE_Y[0]), style, Baseline::Top).draw(d)?;

    let mut soc_line: String<20> = String::new();
    let _ = soc_line.push_str("SOC ");
    fmt_deci_1dp(&mut soc_line, snap.battery_soc_dpct);
    let _ = soc_line.push('%');
    Text::with_baseline(&soc_line, Point::new(0, LINE_Y[1]), style, Baseline::Top).draw(d)?;

    let mut vi_line: String<24> = String::new();
    fmt_centi_1dp(&mut vi_line, snap.pack_voltage_cv);
    let _ = vi_line.push_str("V ");
    fmt_centi_1dp_signed(&mut vi_line, snap.pack_current_ca);
    let _ = vi_line.push('A');
    Text::with_baseline(&vi_line, Point::new(0, LINE_Y[2]), style, Baseline::Top).draw(d)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use embedded_graphics::mock_display::MockDisplay;

    use super::*;

    /// See `impl_network_screen.rs::tests::expected_display`'s doc for what this oracle
    /// methodology proves and its documented 64 px `MockDisplay` window caveat.
    fn expected_display(lines: [&str; 3]) -> MockDisplay<BinaryColor> {
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
    fn renders_zeroed_snapshot() {
        let snap = UiSnapshot::default();

        let mut actual: MockDisplay<BinaryColor> = MockDisplay::new();
        actual.set_allow_out_of_bounds_drawing(true);
        render_battery_screen(&mut actual, &snap).unwrap();

        actual.assert_eq(&expected_display(["BATTERY", "SOC 0.0%", "0.0V 0.0A"]));
    }

    #[test]
    fn renders_charged_battery_with_discharging_current() {
        let snap = UiSnapshot {
            battery_soc_dpct: 875,
            pack_voltage_cv: 5451,
            pack_current_ca: -144,
            ..UiSnapshot::default()
        };

        let mut actual: MockDisplay<BinaryColor> = MockDisplay::new();
        actual.set_allow_out_of_bounds_drawing(true);
        render_battery_screen(&mut actual, &snap).unwrap();

        actual.assert_eq(&expected_display(["BATTERY", "SOC 87.5%", "54.5V -1.4A"]));
    }

    #[test]
    fn rail_draw_fields_do_not_affect_the_battery_screen() {
        // The whole point of this correction: INA3221 rail-draw data must never appear on the
        // Battery screen (it belongs on About — see `impl_about_screen.rs`). Setting non-zero
        // rail fields must render byte-identical output to the zeroed snapshot.
        let snap = UiSnapshot {
            rail_eth_ma: 120,
            rail_radio_ma: 45,
            ..UiSnapshot::default()
        };

        let mut actual: MockDisplay<BinaryColor> = MockDisplay::new();
        actual.set_allow_out_of_bounds_drawing(true);
        render_battery_screen(&mut actual, &snap).unwrap();

        actual.assert_eq(&expected_display(["BATTERY", "SOC 0.0%", "0.0V 0.0A"]));
    }
}

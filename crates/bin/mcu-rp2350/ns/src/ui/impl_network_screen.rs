//! Render body for [`Screen::Network`](super::types::Screen::Network) — IP/DHCP/broker status.
//!
//! Split into its own `#[inline(never)]` fn (not inlined into `Screen::render`'s dispatch match)
//! per the Cortex-M frame hazard documented on [`ScreenView::render`](super::types::ScreenView::render):
//! Rust reserves the max stack frame across all `match` arms, so this fn's locals must not tax the
//! other three screens' calls.

use embedded_graphics::mono_font::ascii::FONT_6X10;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;
use embedded_graphics::text::{Baseline, Text};
use heapless::String;

use super::impl_fmt::fmt_ipv4;
use super::types::{DhcpState, UiSnapshot};

/// Top-left y of each of the four text lines. FONT_6X10 glyphs are 10 px tall; an 8 px pitch packs
/// four lines into the 32 px display height at the cost of a small (2 px) inter-line overlap — an
/// intentional, terse-HUD trade-off (all-caps + digits content has little to no descender/ascender
/// collision in practice), not an oversight.
const LINE_Y: [i32; 4] = [0, 8, 16, 24];

#[inline(never)]
pub(super) fn render_network_screen<D: DrawTarget<Color = BinaryColor>>(
    d: &mut D,
    snap: &UiSnapshot,
) -> Result<(), D::Error> {
    let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);

    Text::with_baseline("NETWORK", Point::new(0, LINE_Y[0]), style, Baseline::Top).draw(d)?;

    let dhcp_line = match snap.dhcp_state {
        DhcpState::Down => "DHCP DOWN",
        DhcpState::Requesting => "DHCP REQ",
        DhcpState::Bound => "DHCP BOUND",
    };
    Text::with_baseline(dhcp_line, Point::new(0, LINE_Y[1]), style, Baseline::Top).draw(d)?;

    let mut ip_line: String<24> = String::new();
    match snap.ip_addr {
        Some(octets) => {
            let _ = ip_line.push_str("IP ");
            fmt_ipv4(&mut ip_line, octets);
        }
        None => {
            let _ = ip_line.push_str("IP --");
        }
    }
    Text::with_baseline(&ip_line, Point::new(0, LINE_Y[2]), style, Baseline::Top).draw(d)?;

    let broker_line = if snap.broker_up {
        "BROKER UP"
    } else {
        "BROKER DOWN"
    };
    Text::with_baseline(broker_line, Point::new(0, LINE_Y[3]), style, Baseline::Top).draw(d)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use embedded_graphics::mock_display::MockDisplay;

    use super::*;

    /// Builds the "expected" MockDisplay by hand-invoking the exact same embedded-graphics
    /// primitives (`Text::with_baseline` + `FONT_6X10`) at the exact strings/positions the
    /// implementation is expected to draw. Comparing two `MockDisplay`s pixel-for-pixel is this
    /// crate's own idiomatic test oracle (see its `diff`/`assert_eq` docs) — it proves content and
    /// layout regressions even though it can't independently prove font glyph shapes (those are
    /// `embedded-graphics`' own tested concern, not this module's).
    ///
    /// Caveat (documented, not hidden): `MockDisplay` is hard-capped at 64x64 px — narrower than
    /// this screen's real 128 px width. Both `expected` and `actual` are built with
    /// `set_allow_out_of_bounds_drawing(true)`, so pixels at x >= 64 are silently, IDENTICALLY
    /// dropped on both sides; content past column 64 cannot be distinguished by this test.
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
    fn renders_bound_dhcp_with_ip_and_broker_up() {
        let snap = UiSnapshot {
            dhcp_state: DhcpState::Bound,
            ip_addr: Some([10, 0, 0, 5]),
            broker_up: true,
            ..UiSnapshot::default()
        };

        let mut actual: MockDisplay<BinaryColor> = MockDisplay::new();
        actual.set_allow_out_of_bounds_drawing(true);
        render_network_screen(&mut actual, &snap).unwrap();

        actual.assert_eq(&expected_display([
            "NETWORK",
            "DHCP BOUND",
            "IP 10.0.0.5",
            "BROKER UP",
        ]));
    }

    #[test]
    fn renders_down_dhcp_with_no_ip_and_broker_down() {
        let snap = UiSnapshot::default();

        let mut actual: MockDisplay<BinaryColor> = MockDisplay::new();
        actual.set_allow_out_of_bounds_drawing(true);
        render_network_screen(&mut actual, &snap).unwrap();

        actual.assert_eq(&expected_display([
            "NETWORK",
            "DHCP DOWN",
            "IP --",
            "BROKER DOWN",
        ]));
    }
}

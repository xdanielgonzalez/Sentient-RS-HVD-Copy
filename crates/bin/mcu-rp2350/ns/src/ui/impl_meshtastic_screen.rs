//! Render body for [`Screen::Meshtastic`](super::types::Screen::Meshtastic) — mesh node number,
//! frames-sent counter, and last-send status. See `impl_network_screen.rs`'s module doc for why
//! this render body lives in its own `#[inline(never)]` fn (Cortex-M frame hazard).

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
pub(super) fn render_meshtastic_screen<D: DrawTarget<Color = BinaryColor>>(
    d: &mut D,
    snap: &UiSnapshot,
) -> Result<(), D::Error> {
    let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);

    Text::with_baseline("MESHTASTIC", Point::new(0, LINE_Y[0]), style, Baseline::Top).draw(d)?;

    let mut node_line: String<20> = String::new();
    if snap.mesh_node_num == 0 {
        let _ = node_line.push_str("NODE ----");
    } else {
        let _ = write!(node_line, "NODE !{:08x}", snap.mesh_node_num);
    }
    Text::with_baseline(&node_line, Point::new(0, LINE_Y[1]), style, Baseline::Top).draw(d)?;

    let mut sent_line: String<20> = String::new();
    let _ = write!(sent_line, "SENT {}", snap.mesh_frames_sent);
    Text::with_baseline(&sent_line, Point::new(0, LINE_Y[2]), style, Baseline::Top).draw(d)?;

    let last_line = if snap.mesh_last_send_ok {
        "LAST OK"
    } else {
        "LAST FAIL"
    };
    Text::with_baseline(last_line, Point::new(0, LINE_Y[3]), style, Baseline::Top).draw(d)?;

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
    fn renders_unknown_node_with_no_frames_sent() {
        let snap = UiSnapshot::default();

        let mut actual: MockDisplay<BinaryColor> = MockDisplay::new();
        actual.set_allow_out_of_bounds_drawing(true);
        render_meshtastic_screen(&mut actual, &snap).unwrap();

        actual.assert_eq(&expected_display([
            "MESHTASTIC",
            "NODE ----",
            "SENT 0",
            "LAST FAIL",
        ]));
    }

    #[test]
    fn renders_known_node_with_frames_sent_and_last_ok() {
        let snap = UiSnapshot {
            mesh_node_num: 0x1a2b_3c4d,
            mesh_frames_sent: 42,
            mesh_last_send_ok: true,
            ..UiSnapshot::default()
        };

        let mut actual: MockDisplay<BinaryColor> = MockDisplay::new();
        actual.set_allow_out_of_bounds_drawing(true);
        render_meshtastic_screen(&mut actual, &snap).unwrap();

        actual.assert_eq(&expected_display([
            "MESHTASTIC",
            "NODE !1a2b3c4d",
            "SENT 42",
            "LAST OK",
        ]));
    }
}

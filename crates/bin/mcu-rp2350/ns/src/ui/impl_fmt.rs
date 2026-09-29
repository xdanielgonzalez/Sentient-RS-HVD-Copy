//! Shared, allocation-free fixed-point/IPv4 text formatting used by the four
//! `impl_*_screen.rs` renderers. Not an `impl` block on a single type (there is no one type these
//! belong to) — named `impl_fmt.rs` anyway to stay inside this task's fixed `ui/impl_*.rs` file
//! ownership glob rather than adding a bare `fmt.rs` outside it. Grouped here once instead of
//! duplicated per screen.
//!
//! Every helper writes into a caller-owned `heapless::String<N>` (already reserved on that
//! screen's own stack frame) — no allocation, no shared scratch buffer to race.

use core::fmt::Write as _;

use heapless::String;

/// Format a deci-scaled value (e.g. `UiSnapshot::battery_soc_dpct`, ×0.1 units) as one decimal
/// place, e.g. `875` -> `"87.5"`. Truncates (no rounding) — terse status display, not a lab readout.
pub(super) fn fmt_deci_1dp<const N: usize>(buf: &mut String<N>, raw: u16) {
    let whole = raw / 10;
    let frac = raw % 10;
    let _ = write!(buf, "{whole}.{frac}");
}

/// Format a centi-scaled value (e.g. `UiSnapshot::pack_voltage_cv`, ×0.01 units) as one decimal
/// place, e.g. `5451` -> `"54.5"`. Truncates (no rounding), same terseness trade-off as
/// [`fmt_deci_1dp`].
pub(super) fn fmt_centi_1dp<const N: usize>(buf: &mut String<N>, raw: u16) {
    let whole = raw / 100;
    let frac = (raw % 100) / 10;
    let _ = write!(buf, "{whole}.{frac}");
}

/// Signed variant of [`fmt_centi_1dp`] for `UiSnapshot::pack_current_ca` (sign = vendor charge
/// convention: positive = charging).
pub(super) fn fmt_centi_1dp_signed<const N: usize>(buf: &mut String<N>, raw: i16) {
    if raw < 0 {
        let _ = buf.push('-');
    }
    fmt_centi_1dp(buf, raw.unsigned_abs());
}

/// Format an IPv4 address as `a.b.c.d`.
pub(super) fn fmt_ipv4<const N: usize>(buf: &mut String<N>, octets: [u8; 4]) {
    let _ = write!(
        buf,
        "{}.{}.{}.{}",
        octets[0], octets[1], octets[2], octets[3]
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deci_1dp_formats_soc() {
        let mut buf: String<8> = String::new();
        fmt_deci_1dp(&mut buf, 875);
        assert_eq!(buf.as_str(), "87.5");
    }

    #[test]
    fn centi_1dp_formats_voltage() {
        let mut buf: String<8> = String::new();
        fmt_centi_1dp(&mut buf, 5451);
        assert_eq!(buf.as_str(), "54.5");
    }

    #[test]
    fn centi_1dp_signed_formats_negative_current() {
        let mut buf: String<8> = String::new();
        fmt_centi_1dp_signed(&mut buf, -144);
        assert_eq!(buf.as_str(), "-1.4");
    }

    #[test]
    fn centi_1dp_signed_formats_positive_current() {
        let mut buf: String<8> = String::new();
        fmt_centi_1dp_signed(&mut buf, 144);
        assert_eq!(buf.as_str(), "1.4");
    }

    #[test]
    fn ipv4_formats_octets() {
        let mut buf: String<16> = String::new();
        fmt_ipv4(&mut buf, [10, 0, 0, 5]);
        assert_eq!(buf.as_str(), "10.0.0.5");
    }
}

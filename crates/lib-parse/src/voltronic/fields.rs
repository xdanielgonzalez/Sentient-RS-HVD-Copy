//! Voltronic/PI30 field decoders — turn an already-`validate_frame`-checked response's ASCII
//! payload into canonical inverter types. Pure, `no_std`, no floats: every fixed-point field is
//! parsed straight from its decimal-ASCII form into a scaled integer (the suffix on the canonical
//! field name states the scale, e.g. `_dv` = deci-volt, `_cv` = centi-volt).
//!
//! CONTRACT: `payload` is the response with the leading `(` and the trailing 2 CRC bytes + `\r`
//! already stripped (i.e. `str::from_utf8(&bytes[1..bytes.len() - 3])`), per the protocol facts:
//! "Parse payload = bytes[1..n-3] split on b' '".
//!
//! PROVISIONAL — every field layout below is derived from the legacy Python reference + protocol
//! docs, not a real Voltronic/QPIGS wire capture (decision 0001's open risk).

use crate::canonical::{
    CanonicalInverterFault, CanonicalInverterReading, InverterMode, InverterRatings,
};
use crate::vendor::ParseError;

/// Parse an unsigned decimal ASCII field (at most one `.`) into a fixed-point integer scaled by
/// `10^decimals` (e.g. `"230.0"` with `decimals = 1` -> `2300`; `"51.20"` with `decimals = 2` ->
/// `5120`). A fractional part shorter than `decimals` is right-zero-padded; longer is rejected.
fn parse_u16_scaled(field: &str, decimals: u32) -> Result<u16, ParseError> {
    let (int_part, frac_part) = match field.split_once('.') {
        Some((i, f)) => (i, f),
        None => (field, ""),
    };
    if int_part.is_empty() || frac_part.len() > decimals as usize {
        return Err(ParseError::Malformed);
    }

    let int_val: u32 = int_part.parse().map_err(|_| ParseError::Malformed)?;
    let frac_val: u32 = if frac_part.is_empty() {
        0
    } else {
        frac_part.parse().map_err(|_| ParseError::Malformed)?
    };
    let pad = decimals as usize - frac_part.len();
    let frac_scale = 10u32.checked_pow(pad as u32).ok_or(ParseError::Malformed)?;
    let frac_val = frac_val
        .checked_mul(frac_scale)
        .ok_or(ParseError::Malformed)?;

    let scale = 10u32.checked_pow(decimals).ok_or(ParseError::Malformed)?;
    let scaled = int_val
        .checked_mul(scale)
        .and_then(|v| v.checked_add(frac_val))
        .ok_or(ParseError::Malformed)?;
    u16::try_from(scaled).map_err(|_| ParseError::Malformed)
}

/// As [`parse_u16_scaled`], but accepts a leading `-` and returns `i16`.
fn parse_i16_scaled(field: &str, decimals: u32) -> Result<i16, ParseError> {
    match field.strip_prefix('-') {
        Some(rest) => {
            let magnitude = i32::from(parse_u16_scaled(rest, decimals)?);
            i16::try_from(-magnitude).map_err(|_| ParseError::Malformed)
        }
        None => {
            let magnitude = parse_u16_scaled(field, decimals)?;
            i16::try_from(magnitude).map_err(|_| ParseError::Malformed)
        }
    }
}

fn next_field<'a>(fields: &mut core::str::Split<'a, char>) -> Result<&'a str, ParseError> {
    fields.next().ok_or(ParseError::Malformed)
}

/// Decode an 8-char binary-digit status-bits field into `status_bits`, where bit `n` of the
/// output is character `7 - n` of the field (i.e. the string is MSB-first: `char[0]` is bit 7).
fn parse_status_bits(field: &str) -> Result<u8, ParseError> {
    let bytes = field.as_bytes();
    if bytes.len() != 8 {
        return Err(ParseError::Malformed);
    }
    let mut bits: u8 = 0;
    for (i, &b) in bytes.iter().enumerate() {
        let bit = match b {
            b'0' => 0,
            b'1' => 1,
            _ => return Err(ParseError::Malformed),
        };
        bits |= bit << (7 - i);
    }
    Ok(bits)
}

/// Decode a `QPIGS`-style status payload (21 space-separated fields; see the protocol facts for
/// the full 1-based layout) into a canonical inverter reading. `mode` and `ratings` are NOT
/// carried by this response — left at their "not yet known" placeholders
/// ([`InverterMode::Unknown`] / `None`); merging in a separately polled mode/ratings result is the
/// caller's concern. Field 21 is tolerated but not required (some firmware omits it).
pub fn parse_status(payload: &str) -> Result<CanonicalInverterReading, ParseError> {
    let mut fields = payload.split(' ');

    let grid_voltage_dv = parse_u16_scaled(next_field(&mut fields)?, 1)?;
    let grid_frequency_dhz = parse_u16_scaled(next_field(&mut fields)?, 1)?;
    let ac_output_voltage_dv = parse_u16_scaled(next_field(&mut fields)?, 1)?;
    let ac_output_frequency_dhz = parse_u16_scaled(next_field(&mut fields)?, 1)?;
    let ac_output_apparent_power_va = parse_u16_scaled(next_field(&mut fields)?, 0)?;
    let ac_output_active_power_w = parse_u16_scaled(next_field(&mut fields)?, 0)?;
    let _field7_skip = next_field(&mut fields)?;
    let bus_voltage_v = parse_u16_scaled(next_field(&mut fields)?, 0)?;
    let battery_voltage_cv = parse_u16_scaled(next_field(&mut fields)?, 2)?;
    let battery_charging_current_a = parse_u16_scaled(next_field(&mut fields)?, 0)?;
    let _field11_skip = next_field(&mut fields)?;
    let inverter_temperature_c = parse_i16_scaled(next_field(&mut fields)?, 0)?;
    let pv_current_da = parse_u16_scaled(next_field(&mut fields)?, 1)?;
    let pv_voltage_dv = parse_u16_scaled(next_field(&mut fields)?, 1)?;
    let _field15_skip = next_field(&mut fields)?;
    let battery_discharge_current_a = parse_u16_scaled(next_field(&mut fields)?, 0)?;
    let status_bits = parse_status_bits(next_field(&mut fields)?)?;
    let _field18_skip = next_field(&mut fields)?;
    let _field19_skip = next_field(&mut fields)?;
    let pv_charging_power_w = parse_u16_scaled(next_field(&mut fields)?, 0)?;
    let _field21_skip_if_present = fields.next();

    Ok(CanonicalInverterReading {
        grid_voltage_dv,
        grid_frequency_dhz,
        ac_output_voltage_dv,
        ac_output_frequency_dhz,
        ac_output_apparent_power_va,
        ac_output_active_power_w,
        bus_voltage_v,
        battery_voltage_cv,
        battery_charging_current_a,
        battery_discharge_current_a,
        inverter_temperature_c,
        pv_current_da,
        pv_voltage_dv,
        pv_charging_power_w,
        status_bits,
        mode: InverterMode::Unknown,
        ratings: None,
    })
}

/// Decode a `QMOD`-style single-character mode payload. Any character outside `P|S|L|B|F|H`
/// decodes to [`InverterMode::Unknown`] (never an error — an unrecognized mode is still valid
/// information, not a framing/parse failure).
pub fn parse_mode(payload: &str) -> Result<InverterMode, ParseError> {
    let field = next_field(&mut payload.split(' '))?;
    Ok(match field.as_bytes().first() {
        Some(b'P') => InverterMode::PowerOn,
        Some(b'S') => InverterMode::Standby,
        Some(b'L') => InverterMode::Line,
        Some(b'B') => InverterMode::Battery,
        Some(b'F') => InverterMode::Fault,
        Some(b'H') => InverterMode::PowerSaving,
        _ => InverterMode::Unknown,
    })
}

/// Decode a `QPIWS`-style binary-digit fault/warning payload. `bit index = string position
/// (0-based)`; `raw_bits` bit `i` = character `i` (LSB-first by position, i.e. NOT reversed like
/// `parse_status_bits`'s QPIGS status-bits field — a different Voltronic sub-protocol
/// convention). Requires the payload to be at least long enough to cover every mapped index
/// (0..=17); a shorter payload is malformed rather than silently reading trailing flags as clear
/// (unlike Seplos's empirical byte-map, this bit map has documented fixed positions).
pub fn parse_fault(payload: &str) -> Result<CanonicalInverterFault, ParseError> {
    let field = next_field(&mut payload.split(' '))?;
    let bytes = field.as_bytes();

    let bit = |idx: usize| -> Result<bool, ParseError> {
        match bytes.get(idx) {
            Some(b'0') => Ok(false),
            Some(b'1') => Ok(true),
            _ => Err(ParseError::Malformed),
        }
    };

    let mut raw_bits: u64 = 0;
    for (i, &b) in bytes.iter().enumerate().take(64) {
        match b {
            b'0' => {}
            b'1' => raw_bits |= 1u64 << i,
            _ => return Err(ParseError::Malformed),
        }
    }

    Ok(CanonicalInverterFault {
        line_fail: bit(5)?,
        overtemperature_fault: bit(9)?,
        fan_locked_fault: bit(10)?,
        battery_low_fault: bit(12)?,
        overload_fault: bit(15)?,
        output_short_fault: bit(17)?,
        raw_bits,
    })
}

/// Decode a `QPIRI`-style whitespace-separated ratings payload: field 7 (1-based) is stored
/// verbatim into `variant` (the rated-active-power string; Voltronic model hints like `3000` ->
/// `PIP-3048LV-MK` exist but are NOT resolved here — no real captures confirm the mapping is
/// reliable across devices, so the raw field is kept and left for a downstream/future
/// improvement); field 13 maps to `battery_type`; field 22 maps to `output_mode`.
pub fn parse_ratings(payload: &str) -> Result<InverterRatings, ParseError> {
    let mut variant_raw: Option<&str> = None;
    let mut battery_type_raw: Option<&str> = None;
    let mut output_mode_raw: Option<&str> = None;

    for (i, field) in payload.split(' ').enumerate() {
        match i + 1 {
            7 => variant_raw = Some(field),
            13 => battery_type_raw = Some(field),
            22 => output_mode_raw = Some(field),
            _ => {}
        }
    }

    let variant_raw = variant_raw.ok_or(ParseError::Malformed)?;
    let battery_type_raw = battery_type_raw.ok_or(ParseError::Malformed)?;
    let output_mode_raw = output_mode_raw.ok_or(ParseError::Malformed)?;

    let mut variant = heapless::String::<16>::new();
    variant
        .push_str(variant_raw)
        .map_err(|_| ParseError::CapacityExceeded)?;

    let mut battery_type = heapless::String::<16>::new();
    battery_type
        .push_str(match battery_type_raw {
            "0" => "agm",
            "1" => "flooded",
            "2" => "user_defined",
            "3" => "lithium",
            _ => "unknown",
        })
        .map_err(|_| ParseError::CapacityExceeded)?;

    let mut output_mode = heapless::String::<24>::new();
    output_mode
        .push_str(match output_mode_raw {
            "00" | "000" => "single_phase",
            "01" => "parallel_single_phase",
            "02" => "three_phase_p1",
            "10" => "three_phase_p2",
            "20" => "three_phase_p3",
            _ => "unknown",
        })
        .map_err(|_| ParseError::CapacityExceeded)?;

    Ok(InverterRatings {
        battery_type,
        variant,
        output_mode,
    })
}

/// Decode a `QID`-style payload: the whole (trimmed) payload IS the serial number.
pub fn parse_serial(payload: &str, out: &mut heapless::String<32>) -> Result<(), ParseError> {
    out.clear();
    out.push_str(payload.trim())
        .map_err(|_| ParseError::CapacityExceeded)?;
    Ok(())
}

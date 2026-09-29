//! Seplos `0x42` telemetry decode → [`CanonicalReading`].
//!
//! INFO binary layout (big-endian throughout), from the verified [`super::frame::ResponseFrame`]:
//! byte 0 `data_flag` u8 (skip) · byte 1 `command_group` u8 (skip) · byte 2 `num_cells` N u8 ·
//! then N×u16 cell voltages (millivolts) · `num_temps` T u8 · then T×u16 temperatures (raw
//! Kelvin×10) · then a fixed 19-byte block in order:
//! `current i16` (the ONLY signed field) · `total_voltage u16` · `remaining_capacity u16` ·
//! `custom_number u8` (skip) · `typical_capacity u16` · `soc u16` · `rated_capacity u16` ·
//! `cycles u16` · `soh u16` · `port_voltage u16`.
//!
//! Raw fixed-point units are preserved (no floats); scaling is a downstream concern. Bounds:
//! N ≤ [`crate::canonical::MAX_CELLS`], T ≤ [`crate::canonical::MAX_TEMPS`] (else
//! `TooManyElements`); underrun anywhere → `InfoUnderrun`.

use super::error::SeplosError;
use crate::canonical::CanonicalReading;

/// Decode a verified Seplos `0x42` telemetry frame into the vendor-neutral canonical reading.
///
/// FILL: implement per the module-doc layout. Read `num_cells`/`num_temps` from the payload
/// (variable), big-endian `u16`/`i16` via `from_be_bytes`, push cells/temps into the bounded
/// `heapless::Vec`s (map a full vec to `SeplosError::TooManyElements`), bounds-check every slice
/// access and return `SeplosError::InfoUnderrun` on any shortfall. `frame.adr` → `device_address`.
/// No `unwrap`/`expect`, no panic, no indexing that can panic.
pub fn decode_reading(
    frame: &super::frame::ResponseFrame,
) -> Result<CanonicalReading, SeplosError> {
    // Skip bytes 0–1 (data_flag, command_group); num_cells is at byte 2.
    let mut offset = 2usize;

    // Read num_cells at byte 2
    let num_cells = frame
        .info
        .get(offset..offset + 1)
        .ok_or(SeplosError::InfoUnderrun)?[0] as usize;
    offset += 1;

    if num_cells > crate::canonical::MAX_CELLS {
        return Err(SeplosError::TooManyElements);
    }

    // Read N × u16 cell voltages (big-endian)
    let mut cell_voltages_mv = heapless::Vec::new();
    for _ in 0..num_cells {
        let bytes = frame
            .info
            .get(offset..offset + 2)
            .ok_or(SeplosError::InfoUnderrun)?;
        cell_voltages_mv
            .push(u16::from_be_bytes([bytes[0], bytes[1]]))
            .map_err(|_| SeplosError::TooManyElements)?;
        offset += 2;
    }

    // Read num_temps
    let num_temps = frame
        .info
        .get(offset..offset + 1)
        .ok_or(SeplosError::InfoUnderrun)?[0] as usize;
    offset += 1;

    if num_temps > crate::canonical::MAX_TEMPS {
        return Err(SeplosError::TooManyElements);
    }

    // Read T × u16 temperatures (big-endian, raw)
    let mut temperatures_raw = heapless::Vec::new();
    for _ in 0..num_temps {
        let bytes = frame
            .info
            .get(offset..offset + 2)
            .ok_or(SeplosError::InfoUnderrun)?;
        temperatures_raw
            .push(u16::from_be_bytes([bytes[0], bytes[1]]))
            .map_err(|_| SeplosError::TooManyElements)?;
        offset += 2;
    }

    // Fixed 19-byte block: current (i16, signed) + 9 more fields
    let current_bytes = frame
        .info
        .get(offset..offset + 2)
        .ok_or(SeplosError::InfoUnderrun)?;
    let current_ca = i16::from_be_bytes([current_bytes[0], current_bytes[1]]);
    offset += 2;

    let total_voltage_bytes = frame
        .info
        .get(offset..offset + 2)
        .ok_or(SeplosError::InfoUnderrun)?;
    let total_voltage_cv = u16::from_be_bytes([total_voltage_bytes[0], total_voltage_bytes[1]]);
    offset += 2;

    let remaining_capacity_bytes = frame
        .info
        .get(offset..offset + 2)
        .ok_or(SeplosError::InfoUnderrun)?;
    let remaining_capacity_cah =
        u16::from_be_bytes([remaining_capacity_bytes[0], remaining_capacity_bytes[1]]);
    offset += 2;

    // Skip custom_number (u8)
    let _ = frame
        .info
        .get(offset..offset + 1)
        .ok_or(SeplosError::InfoUnderrun)?;
    offset += 1;

    let typical_capacity_bytes = frame
        .info
        .get(offset..offset + 2)
        .ok_or(SeplosError::InfoUnderrun)?;
    let typical_capacity_cah =
        u16::from_be_bytes([typical_capacity_bytes[0], typical_capacity_bytes[1]]);
    offset += 2;

    let soc_bytes = frame
        .info
        .get(offset..offset + 2)
        .ok_or(SeplosError::InfoUnderrun)?;
    let soc_dpct = u16::from_be_bytes([soc_bytes[0], soc_bytes[1]]);
    offset += 2;

    let rated_capacity_bytes = frame
        .info
        .get(offset..offset + 2)
        .ok_or(SeplosError::InfoUnderrun)?;
    let rated_capacity_cah = u16::from_be_bytes([rated_capacity_bytes[0], rated_capacity_bytes[1]]);
    offset += 2;

    let cycles_bytes = frame
        .info
        .get(offset..offset + 2)
        .ok_or(SeplosError::InfoUnderrun)?;
    let cycles = u16::from_be_bytes([cycles_bytes[0], cycles_bytes[1]]);
    offset += 2;

    let soh_bytes = frame
        .info
        .get(offset..offset + 2)
        .ok_or(SeplosError::InfoUnderrun)?;
    let soh_dpct = u16::from_be_bytes([soh_bytes[0], soh_bytes[1]]);
    offset += 2;

    let port_voltage_bytes = frame
        .info
        .get(offset..offset + 2)
        .ok_or(SeplosError::InfoUnderrun)?;
    let port_voltage_cv = u16::from_be_bytes([port_voltage_bytes[0], port_voltage_bytes[1]]);

    Ok(CanonicalReading {
        device_address: frame.adr,
        cell_voltages_mv,
        temperatures_raw,
        current_ca,
        total_voltage_cv,
        remaining_capacity_cah,
        typical_capacity_cah,
        rated_capacity_cah,
        soc_dpct,
        soh_dpct,
        cycles,
        port_voltage_cv,
    })
}

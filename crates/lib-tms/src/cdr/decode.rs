//! Minimal XCDR2 (DDS-XTypes 1.3) decode for the two inbound TMS **request** types the device must
//! accept: [`crate::generated::EnergyStartStopRequest`] and [`crate::generated::PowerSwitchRequest`].
//!
//! Scope is deliberately narrow (per the render-on-output memory model, the device only *reads*
//! requests; it *renders* every outbound topic). The full generated decode surface is out of scope
//! for this hand-written core — the codegen (WP3) owns the encode chains; this file owns the two
//! decoders needed to act on operator commands.
//!
//! # Wire shape (both types are `@extensibility(APPENDABLE)`, XCDR2, little-endian)
//!
//! ```text
//! [encap id 2B = D_CDR2_LE {0x00,0x09}] [options 2B] -- DDS-XTypes 1.3 §7.6.3.1.2 Table 60
//! [DHEADER u32]                                       -- top-level APPENDABLE, rule (1)→(30)
//!   <members in IDL declaration order, each nested APPENDABLE struct carrying its own DHEADER>
//! ```

use super::types::{CdrError, CdrReader};
use crate::generated as g;

/// Read + validate the 4-byte RTPS encapsulation header, accepting only the little-endian XCDR2
/// encapsulations (`CDR2_LE` `{0x00,0x07}`, `D_CDR2_LE` `{0x00,0x09}`; DDS-XTypes 1.3 §7.6.3.1.2
/// Table 60). Leaves the cursor at offset 4 (the start of the payload; alignment origin).
fn read_encapsulation(r: &mut CdrReader) -> Result<(), CdrError> {
    let hi = r.u8()?;
    let lo = r.u8()?;
    let _options = (r.u8()?, r.u8()?); // 2-byte options; padding bits not needed for decode
    match (hi, lo) {
        (0x00, 0x07) | (0x00, 0x09) => Ok(()),
        _ => Err(CdrError::BadEncapsulation),
    }
}

fn config_id(v: u32) -> Result<g::ConfigId, CdrError> {
    Ok(match v {
        0 => g::ConfigId::Unknown,
        1 => g::ConfigId::Defaults,
        2 => g::ConfigId::Active,
        3 => g::ConfigId::OnReboot,
        4 => g::ConfigId::OnCommsLoss,
        _ => return Err(CdrError::BadEnum),
    })
}

fn energy_level(v: u32) -> Result<g::EnergyStartStopLevel, CdrError> {
    Ok(match v {
        0 => g::EnergyStartStopLevel::Unknown,
        1 => g::EnergyStartStopLevel::Any,
        2 => g::EnergyStartStopLevel::Off,
        3 => g::EnergyStartStopLevel::Warm,
        4 => g::EnergyStartStopLevel::Idle,
        5 => g::EnergyStartStopLevel::Ready,
        6 => g::EnergyStartStopLevel::ReadySynced,
        7 => g::EnergyStartStopLevel::Operational,
        _ => return Err(CdrError::BadEnum),
    })
}

fn continuity(v: u32) -> Result<g::DesiredCircuitContinuity, CdrError> {
    Ok(match v {
        0 => g::DesiredCircuitContinuity::Unknown,
        1 => g::DesiredCircuitContinuity::Open,
        2 => g::DesiredCircuitContinuity::Closed,
        3 => g::DesiredCircuitContinuity::SyncClosed,
        4 => g::DesiredCircuitContinuity::ClosedInternal,
        5 => g::DesiredCircuitContinuity::NoChange,
        _ => return Err(CdrError::BadEnum),
    })
}

fn read_string32(r: &mut CdrReader) -> Result<heapless::String<32>, CdrError> {
    let s = r.string()?;
    heapless::String::try_from(s).map_err(|_| CdrError::CapacityExceeded)
}

/// Decode a nested `PowerSwitchCloseConditions` (APPENDABLE: its own DHEADER + four `f32`).
fn read_switch_conditions(r: &mut CdrReader) -> Result<g::PowerSwitchCloseConditions, CdrError> {
    let _dheader = r.dheader()?;
    Ok(g::PowerSwitchCloseConditions {
        min_voltage: r.f32()?,
        max_voltage: r.f32()?,
        min_frequency: r.f32()?,
        max_frequency: r.f32()?,
    })
}

/// Decode the optional trailing `switch_conditions` member (DDS-XTypes 1.3 §7.4.3.5.2 / rule (20):
/// a `<is_present>` boolean, then the value iff present). This is the read-side counterpart of
/// [`crate::cdr::emit_optional`] — kept in lock-step so a flip of the optional-encoding policy (Risk
/// 2, the CycloneDDS arbitration) changes both sides in one place.
fn read_optional_switch_conditions(
    r: &mut CdrReader,
) -> Result<Option<g::PowerSwitchCloseConditions>, CdrError> {
    if r.boolean()? {
        Ok(Some(read_switch_conditions(r)?))
    } else {
        Ok(None)
    }
}

/// Decode an [`crate::generated::EnergyStartStopRequest`] (IDL line 4437) from an XCDR2 payload.
pub fn decode_energy_start_stop_request(
    bytes: &[u8],
) -> Result<g::EnergyStartStopRequest, CdrError> {
    let mut r = CdrReader::new(bytes);
    read_encapsulation(&mut r)?;
    let _top = r.dheader()?;

    // request_id: DeviceConfigRequest (APPENDABLE → own DHEADER)
    let _rid = r.dheader()?;
    let request_id = g::DeviceConfigRequest {
        requesting_device_id: read_string32(&mut r)?,
        target_device_id: read_string32(&mut r)?,
        config: config_id(r.enum_u32()?)?,
    };

    let sequence_id = r.u64()?;
    let from_level = energy_level(r.enum_u32()?)?;
    let to_level = energy_level(r.enum_u32()?)?;
    let switch_conditions = read_optional_switch_conditions(&mut r)?;

    Ok(g::EnergyStartStopRequest {
        request_id,
        sequence_id,
        from_level,
        to_level,
        switch_conditions,
    })
}

/// Decode a [`crate::generated::PowerSwitchRequest`] (IDL line 4084) from an XCDR2 payload.
pub fn decode_power_switch_request(bytes: &[u8]) -> Result<g::PowerSwitchRequest, CdrError> {
    let mut r = CdrReader::new(bytes);
    read_encapsulation(&mut r)?;
    let _top = r.dheader()?;

    // request_id: PowerPortConfigRequest (APPENDABLE → own DHEADER)
    let _rid = r.dheader()?;
    let request_id = g::PowerPortConfigRequest {
        requesting_device_id: read_string32(&mut r)?,
        target_device_id: read_string32(&mut r)?,
        config: config_id(r.enum_u32()?)?,
        port_number: r.i32()?,
    };

    let sequence_id = r.u64()?;
    let continuity = continuity(r.enum_u32()?)?;
    let switch_conditions = read_optional_switch_conditions(&mut r)?;

    Ok(g::PowerSwitchRequest {
        request_id,
        sequence_id,
        continuity,
        switch_conditions,
    })
}

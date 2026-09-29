use super::prelude::*;

// ---------------------------------------------------------------------------------------------
// --dump-fixtures mode: offline pipeline, no networking.
// ---------------------------------------------------------------------------------------------

/// Run the full offline `SeplosSim -> lib_parse -> lib_mapping -> JSON` pipeline and write the
/// three golden fixtures into `dir`. No networking; exits the process with 0 on success (via the
/// normal `main` return path).
pub(super) fn dump_fixtures(dir: &Path) -> Result<(), AppError> {
    fs::create_dir_all(dir).map_err(|e| AppError::Io {
        what: format!("create fixture directory {}", dir.display()),
        source: e,
    })?;

    let device_id = device_identity()?;
    let now = ClockMonotonic {
        seconds: 0,
        nanoseconds: 0,
    };

    // --- 0x42 telemetry: sample_16_cell -> dc_measurement_update + power_hardware_update ---
    let mut telemetry_sim = SeplosSim::new(BatteryState::sample_16_cell());
    let telemetry_reply = telemetry_sim.respond(&build_request(
        0,
        lib_parse::seplos::command::Command::Telemetry,
    )?);
    let telemetry_bytes = reply_bytes(telemetry_reply, "0x42 telemetry")?;
    let reading = Seplos::new()
        .parse_reading(&telemetry_bytes, 0)
        .map_err(|e| {
            AppError::Fixture(format!("parse_reading on sim-encoded 0x42 frame: {e:?}"))
        })?;

    write_rendered_fixture(dir, "dc_measurement_update.golden.json", |buf| {
        render_tms(
            TmsTopic::DcMeasurementUpdate,
            device_id.clone(),
            &reading,
            now,
            buf,
        )
        .ok()
    })?;
    write_rendered_fixture(dir, "power_hardware_update.golden.json", |buf| {
        render_tms(
            TmsTopic::PowerHardwareUpdate,
            device_id.clone(),
            &reading,
            now,
            buf,
        )
        .ok()
    })?;

    // --- 0x44 fault: the documented 4-fault vector -> active_diagnostic_state ---
    let mut fault_state = BatteryState::sample_16_cell();
    fault_state.faults = FaultFlags {
        high_voltage_protection: true,
        short_circuit_protection: true,
        low_capacity_warning: true,
        charge_current_warning: true,
        ..Default::default()
    };
    let mut fault_sim = SeplosSim::new(fault_state);
    let fault_reply = fault_sim.respond(&build_request(
        0,
        lib_parse::seplos::command::Command::Fault,
    )?);
    let fault_bytes = reply_bytes(fault_reply, "0x44 fault")?;
    let fault = Seplos::new()
        .parse_fault(&fault_bytes, 0)
        .map_err(|e| AppError::Fixture(format!("parse_fault on sim-encoded 0x44 frame: {e:?}")))?;

    write_rendered_fixture(dir, "active_diagnostic_state.golden.json", |buf| {
        render_fault(&fault, device_id, now, buf).ok()
    })?;

    eprintln!(
        "seplos-tcp-sim: wrote 3 golden fixtures to {}",
        dir.display()
    );
    Ok(())
}

fn device_identity() -> Result<Identity, AppError> {
    let mut id = Identity::new();
    id.push_str(DEVICE_ID).map_err(|_| {
        AppError::Fixture(format!("device id {DEVICE_ID:?} exceeds Identity capacity"))
    })?;
    Ok(id)
}

fn build_request(
    dip: u8,
    cmd: lib_parse::seplos::command::Command,
) -> Result<heapless::Vec<u8, 32>, AppError> {
    let mut out = heapless::Vec::new();
    lib_parse::seplos::command::build_command(dip, cmd, &mut out)
        .map_err(|e| AppError::Fixture(format!("build_command: {e:?}")))?;
    Ok(out)
}

fn reply_bytes(
    reply: SimReply,
    what: &str,
) -> Result<heapless::Vec<u8, { test_kit::seplos::MAX_FRAME }>, AppError> {
    match reply {
        SimReply::Bytes(b) => Ok(b),
        SimReply::Silence => Err(AppError::Fixture(format!(
            "SeplosSim unexpectedly returned Silence for {what} (Behavior::Normal expected)"
        ))),
    }
}

/// Render a TMS payload via `render_tms`/`render_fault` (see call sites) — `lib_mapping`'s
/// [`lib_mapping::wire`] flat serializer, the SAME encoder the firmware uses on both the RPi host
/// publish path (`impl_app::host::HostPlatform`) and the device-hosted broker
/// (`bin_zephyr_platform::render_pending_topic`) — and write the bytes to `dir/name`. This guarantees
/// byte/format identity with the firmware's real output, including the `NaN` -> `null` policy —
/// never hand-written JSON. (Previously called through `lib_mapping::codec::to_json_slice`'s
/// serde/`ryu` codec directly; `render_tms`/`render_fault` are now backed by the flat wire
/// serializer instead — see `docs/wire-serialization.md`.)
fn write_rendered_fixture(
    dir: &Path,
    name: &str,
    render: impl FnOnce(&mut [u8]) -> Option<usize>,
) -> Result<(), AppError> {
    let mut buf = [0u8; lib_mapping::sink::MAX_PAYLOAD];
    let n = render(&mut buf)
        .ok_or_else(|| AppError::Fixture(format!("serialize {name}: buffer too small")))?;
    let path = dir.join(name);
    fs::write(&path, &buf[..n]).map_err(|e| AppError::Io {
        what: format!("write {}", path.display()),
        source: e,
    })?;
    Ok(())
}

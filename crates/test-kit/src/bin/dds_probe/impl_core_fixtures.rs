use super::impl_core_render::render;
use super::prelude::*;

fn dev(s: &str) -> Identity {
    let mut id = Identity::new();
    let _ = id.push_str(s);
    id
}

fn hex(b: &[u8]) -> String {
    b.iter()
        .map(|x| format!("{x:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// `dump-fixtures` mode: render the identity-topic fixtures with the SAME canned values
/// `deploy/dds-probe/dump_reference.py`'s `arbitration`/`fixtures` modes use, print `name: hex` per
/// line to stdout — no networking. A quick Rust<->Python cross-check for the topics that don't need
/// the full canonical->TMS mapping pipeline (battery/inverter telemetry topics stay
/// `lib_mapping::render_cdr`'s job, exercised by the byte-oracle tests, not this bin).
pub(crate) fn dump_fixtures() {
    let heartbeat = lib_tms::Heartbeat {
        device_id: dev("METEOR-001"),
        sequence_number: 42,
    };
    println!("heartbeat: {}", hex(&render(&heartbeat)));

    let reply = lib_tms::Reply {
        requesting_device_id: dev("REQ-1"),
        target_device_id: dev("TGT-1"),
        config: lib_tms::ConfigId::Active,
        port_number: 7,
        request_sequence_id: 0x0102_0304_0506_0708,
        status: lib_tms::ReplyStatus {
            code: lib_tms::ReplyCode::NotImplemented,
            reason: heapless::String::try_from("nope").unwrap_or_default(),
        },
    };
    println!("reply: {}", hex(&render(&reply)));

    let essr = lib_tms::EnergyStartStopState {
        device_id: dev("METEOR-001"),
        present_level: lib_tms::EnergyStartStopLevel::Operational,
        future_level: lib_tms::EnergyStartStopLevel::Operational,
        request_lock: false,
        elapsed_time: 0.0,
        remaining_time: f32::NAN,
        present_level_reason: lib_tms::PowerSwitchReason::Startup,
        present_level_actor: None,
        future_level_reason: lib_tms::PowerSwitchTransitionCode::None,
    };
    println!("energy_start_stop_state: {}", hex(&render(&essr)));

    let present = lib_tms::ActiveMicrogridControllerState {
        device_id: dev("MC"),
        master_id: Some(dev("M1")),
    };
    println!(
        "active_microgrid_controller_state(present): {}",
        hex(&render(&present))
    );

    let absent = lib_tms::ActiveMicrogridControllerState {
        device_id: dev("MC"),
        master_id: None,
    };
    println!(
        "active_microgrid_controller_state(absent): {}",
        hex(&render(&absent))
    );
}

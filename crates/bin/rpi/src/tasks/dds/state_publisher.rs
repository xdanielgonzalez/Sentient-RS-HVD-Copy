//! DDS-side four Required STOR state topics (#324, WP6) — the DDS transport counterpart of
//! `crate::tasks::state_publisher`. Same role-scoping + at-start/on-change/60 s-refresh cadence
//! (READ that file's doc comment for the full spec-timing + PROVISIONAL-field rationale — every
//! field decision here matches it exactly). All four topics are spec PublishLast (`generated_qos`
//! maps them to `QosProfile::PublishLast` = `TRANSIENT_LOCAL` + `RELIABLE`), so a late DDS
//! subscriber replays the writer's own history — no application-level "retain" to reason about the
//! way MQTT needed one.
//!
//! None of the four has a DDS patch type yet (`EnergyStartStopState`/`ActiveMicrogridControllerState`
//! carry an `@optional` string; `PowerPortState`/`DcLoadSharingState` nest further `@optional`
//! members — see `lib_dds::dustdds::types::patch`'s module doc comment), so every enqueue here is
//! dropped by `lib_dds::run_dds_stack` today (`DdsError::UnsupportedTopic`, logged, never a crash).
//! Wired now so zero further code changes are needed once a follow-up lands their patch types.

use std::time::Duration;

use lib_dds::DdsHandle;
use lib_mapping::schema::{
    ActiveMicrogridControllerState, CableSenseStatus, ConfigId, DeviceRole, EnergyStartStopLevel,
    EnergyStartStopState, Identity, PowerPortState, PowerPortStatus, PowerSwitchReason,
    PowerSwitchTransitionCode,
};
use lib_tms::TmsTopicId;
use tokio::sync::watch;
use tokio::time::MissedTickBehavior;

use super::util::render_and_send;
use crate::config::IdentityCfg;
use crate::tasks::util::bounded;

/// Slow periodic re-assertion of all four state topics — matches
/// `tasks::state_publisher::STATE_REFRESH_PERIOD`.
const STATE_REFRESH_PERIOD: Duration = Duration::from_secs(60);
/// The single declared power-port number these state publishers describe — matches
/// `tasks::state_publisher::DECLARED_PORT`.
const DECLARED_PORT: i32 = 0;

fn energy_start_stop_state(device_id: &Identity) -> EnergyStartStopState {
    EnergyStartStopState {
        device_id: device_id.clone(),
        present_level: EnergyStartStopLevel::Operational,
        future_level: EnergyStartStopLevel::Operational,
        request_lock: false,
        elapsed_time: 0.0,
        remaining_time: f32::NAN,
        present_level_reason: PowerSwitchReason::Startup,
        present_level_actor: None,
        future_level_reason: PowerSwitchTransitionCode::None,
    }
}

fn power_port_state(device_id: &Identity) -> PowerPortState {
    let mut power_ports = heapless::Vec::new();
    let _ = power_ports.push(PowerPortStatus {
        port_number: DECLARED_PORT,
        cable_status: CableSenseStatus::Unknown,
        cable_id: None,
        connection: None,
        switch_status: None,
    });
    PowerPortState {
        device_id: device_id.clone(),
        power_ports,
    }
}

fn dc_load_sharing_state(device_id: &Identity) -> lib_mapping::schema::DcLoadSharingState {
    lib_mapping::schema::DcLoadSharingState {
        device_id: device_id.clone(),
        config: ConfigId::Unknown,
        parameters: heapless::Vec::new(),
    }
}

fn active_mc_state(
    device_id: &Identity,
    master_id: Option<Identity>,
) -> ActiveMicrogridControllerState {
    ActiveMicrogridControllerState {
        device_id: device_id.clone(),
        master_id,
    }
}

fn send_amc(handle: &DdsHandle, device_id: &Identity, master_id: Option<Identity>) {
    render_and_send(
        handle,
        device_id,
        TmsTopicId::ActiveMicrogridControllerState,
        &active_mc_state(device_id, master_id),
    );
}

/// Publish the role-scoped At-Start/refresh set (everything except `AMCState` — see
/// `tasks::state_publisher::publish_role_set`'s doc comment for why it's split out).
fn send_role_set(handle: &DdsHandle, device_id: &Identity, role: DeviceRole) {
    render_and_send(
        handle,
        device_id,
        TmsTopicId::PowerPortState,
        &power_port_state(device_id),
    );
    if matches!(role, DeviceRole::Storage) {
        render_and_send(
            handle,
            device_id,
            TmsTopicId::EnergyStartStopState,
            &energy_start_stop_state(device_id),
        );
        render_and_send(
            handle,
            device_id,
            TmsTopicId::DcLoadSharingState,
            &dc_load_sharing_state(device_id),
        );
    }
}

/// Publish the four Required STOR state topics over DDS for ONE injected identity (D13), at
/// startup and on change (`AMCState` tracks `mc_rx`), with a slow periodic refresh — mirrors
/// `tasks::state_publisher::run_state_publisher`'s cadence exactly, on the shared `handle` (the
/// same [`DdsHandle`] `DdsTap` uses).
pub async fn run_dds_state_publisher(
    identity: IdentityCfg,
    handle: DdsHandle,
    mut mc_rx: watch::Receiver<Option<Identity>>,
    mut shutdown: watch::Receiver<bool>,
) {
    let device_id: Identity = bounded(&identity.device_id);
    let role = identity.role;

    send_role_set(&handle, &device_id, role);
    send_amc(&handle, &device_id, mc_rx.borrow().clone());

    let mut refresh = tokio::time::interval(STATE_REFRESH_PERIOD);
    refresh.set_missed_tick_behavior(MissedTickBehavior::Delay);
    refresh.tick().await;

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    break;
                }
            }
            changed = mc_rx.changed() => {
                if changed.is_ok() {
                    send_amc(&handle, &device_id, mc_rx.borrow().clone());
                }
            }
            _ = refresh.tick() => {
                send_role_set(&handle, &device_id, role);
                send_amc(&handle, &device_id, mc_rx.borrow().clone());
            }
        }
    }
}

//! The four Required STOR-topic state publishers (#263) — `EnergyStartStopState`, `PowerPortState`,
//! `DcLoadSharingState`, and `ActiveMicrogridControllerState` — closing the gap where these
//! spec-Required topics (Tables B.1/B.2 + the A.8.7 usage conditions) had no publisher at all.
//!
//! **Per identity (D13):** `crate::runtime::supervise` spawns ONE `run_state_publisher` PER derived
//! [`IdentityCfg`], role-scoped:
//! - `EnergyStartStopState` — STOR only ("shall be published by SRC and STOR", B.16).
//! - `PowerPortState` — STOR + CONV ("shall be published by SRC, STOR, DIST, LOAD, CONV", B.17).
//! - `DcLoadSharingState` — STOR only, Conditional-Required (HAS DC PORTS — the battery declares a
//!   DC port; B/A.8.7). The CONV DC-port load-sharing case is out of scope here (PROVISIONAL).
//! - `ActiveMicrogridControllerState` — all power devices (B.13); its `master_id` is fed by the
//!   MC-selection state machine ([`crate::tasks::mc_select`], A.7.5) via an injected `watch`.
//!
//! **Timing/QoS.** All four are spec PublishLast / At Start / On Change. They are published
//! **retained + QoS AtLeastOnce** over the task's own loopback `rumqttc` client (retain works on
//! this TCP client — the in-process-`LinkTx` limitation doesn't apply; see
//! `crate::tasks::util::publish_json_opts`), so a late subscriber gets the current state
//! immediately. `AMCState` is re-published On Change whenever the MC-selection watch changes; a
//! slow periodic refresh re-asserts all four as belt-and-suspenders durability.
//!
//! **Honest content (PROVISIONAL where the device can't source a field).** This adapter proxies a
//! telemetry-only STOR/CONV device with no command surface, no load-sharing, and no per-bus
//! telemetry-health feed reaching task-land yet, so:
//! - `EnergyStartStopState`: `present_level = future_level = ESSL_OPERATIONAL` (the adapter is up
//!   and polling — a running-state proxy; a true health-driven transition to `ESSL_UNKNOWN` needs a
//!   telemetry-health signal that does not cross into task-land today — documented follow-up),
//!   `request_lock = false`, `elapsed_time = 0`, `remaining_time = NaN`, reason `PSR_STARTUP`, no
//!   actor, future reason `PSTC_NONE`.
//! - `PowerPortState`: one `PowerPortStatus` for the single declared port (port 0), `cable_status =
//!   CS_UNKNOWN`, no cable id / connection / switch (no switch on this device).
//! - `DcLoadSharingState`: empty `parameters` (a non-load-sharing battery), `config =
//!   CONFIG_UNKNOWN` — all-NaN-honest.

use std::time::Duration;

use lib_mapping::schema::{
    topics, ActiveMicrogridControllerState, CableSenseStatus, ConfigId, DeviceRole,
    EnergyStartStopLevel, EnergyStartStopState, Identity, PowerPortState, PowerPortStatus,
    PowerSwitchReason, PowerSwitchTransitionCode,
};
use rumqttc::{AsyncClient, MqttOptions, QoS};
use tokio::sync::watch;
use tokio::time::MissedTickBehavior;

use crate::config::IdentityCfg;
use crate::tasks::util::{bounded, publish_json_opts};

/// MQTT keep-alive for this task's own client connection.
const KEEP_ALIVE_SECS: u64 = 30;
/// Slow periodic re-assertion of all four state topics (belt-and-suspenders durability on top of
/// the retained PublishLast publishes — mirrors `DeviceInfo`'s 60 s refresh).
const STATE_REFRESH_PERIOD: Duration = Duration::from_secs(60);

/// The single declared power-port number these state publishers describe (matches the one
/// `PowerPortInfo` each role advertises in its `DeviceInfo` — a documented bounded subset).
const DECLARED_PORT: i32 = 0;

/// `EnergyStartStopState` for a healthy, running STOR device — see the module doc comment for the
/// PROVISIONAL field rationale.
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

/// `PowerPortState` — one `PowerPortStatus` for the single declared port (no cable/connection/switch
/// sensing on this device).
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

/// `DcLoadSharingState` — a non-load-sharing battery: empty `parameters`, config `CONFIG_UNKNOWN`.
fn dc_load_sharing_state(device_id: &Identity) -> lib_mapping::schema::DcLoadSharingState {
    lib_mapping::schema::DcLoadSharingState {
        device_id: device_id.clone(),
        config: ConfigId::Unknown,
        parameters: heapless::Vec::new(),
    }
}

/// `ActiveMicrogridControllerState` with the currently-selected MC id (`None` when no MC is active).
fn active_mc_state(
    device_id: &Identity,
    master_id: Option<Identity>,
) -> ActiveMicrogridControllerState {
    ActiveMicrogridControllerState {
        device_id: device_id.clone(),
        master_id,
    }
}

/// Publish `AMCState` (retained + QoS1) reflecting `master_id`.
fn publish_amc(client: &AsyncClient, device_id: &Identity, master_id: Option<Identity>) {
    publish_json_opts(
        client,
        device_id,
        topics::ACTIVE_MICROGRID_CONTROLLER_STATE,
        &active_mc_state(device_id, master_id),
        QoS::AtLeastOnce,
        true,
    );
}

/// Publish the role-scoped At-Start/refresh set (everything except `AMCState`, which is published
/// separately so it can also fire On-Change from the MC watch).
fn publish_role_set(client: &AsyncClient, device_id: &Identity, role: DeviceRole) {
    let is_stor = matches!(role, DeviceRole::Storage);
    // PowerPortState: STOR + CONV (and any other power role).
    publish_json_opts(
        client,
        device_id,
        topics::POWER_PORT_STATE,
        &power_port_state(device_id),
        QoS::AtLeastOnce,
        true,
    );
    if is_stor {
        publish_json_opts(
            client,
            device_id,
            topics::ENERGY_START_STOP_STATE,
            &energy_start_stop_state(device_id),
            QoS::AtLeastOnce,
            true,
        );
        publish_json_opts(
            client,
            device_id,
            topics::DC_LOAD_SHARING_STATE,
            &dc_load_sharing_state(device_id),
            QoS::AtLeastOnce,
            true,
        );
    }
}

/// Publish the four Required STOR state topics for ONE injected identity (D13), retained + QoS1, at
/// startup and on change (the `AMCState` `master_id` tracks `mc_rx`), with a slow periodic refresh.
pub async fn run_state_publisher(
    identity: IdentityCfg,
    broker_host: String,
    broker_port: u16,
    mut mc_rx: watch::Receiver<Option<Identity>>,
    mut shutdown: watch::Receiver<bool>,
) {
    let device_id: Identity = bounded(&identity.device_id);
    let role = identity.role;
    let mut opts = MqttOptions::new(format!("{device_id}-state"), broker_host, broker_port);
    opts.set_keep_alive(Duration::from_secs(KEEP_ALIVE_SECS));
    let (client, mut eventloop) = AsyncClient::new(opts, 10);

    // At-start: publish the role-scoped set + AMCState (with the current MC selection, if any).
    publish_role_set(&client, &device_id, role);
    publish_amc(&client, &device_id, mc_rx.borrow().clone());

    let mut refresh = tokio::time::interval(STATE_REFRESH_PERIOD);
    refresh.set_missed_tick_behavior(MissedTickBehavior::Delay);
    // `interval` fires its first tick immediately; the at-start publish above already covers it.
    refresh.tick().await;

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    break;
                }
            }
            changed = mc_rx.changed() => {
                // On-Change: the MC-selection machine updated the selected controller.
                if changed.is_ok() {
                    let master = mc_rx.borrow().clone();
                    publish_amc(&client, &device_id, master);
                }
            }
            _ = refresh.tick() => {
                publish_role_set(&client, &device_id, role);
                publish_amc(&client, &device_id, mc_rx.borrow().clone());
            }
            event = eventloop.poll() => {
                // The eventloop must be polled continuously for `try_publish`'d packets to flush.
                if let Err(e) = event {
                    log::debug!("state_publisher: mqtt eventloop error (will retry): {e}");
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            }
        }
    }
}

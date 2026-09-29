//! The MC-selection state machine wired to the embedded broker (MIL-STD-3071 A.7.5, #264): ONE
//! shared task per process that observes every device's MC Heartbeats + DeviceInfo and computes the
//! currently-active microgrid controller, feeding its id to every identity's state publisher (which
//! renders `ActiveMicrogridControllerState`) via a `watch` channel.
//!
//! Selection logic + the New-MC / Lost-Active-MC / No-MC timers live in the pure, host-tested
//! [`McSelector`] (see `crate::tasks::impl_mc_selector`), constructed here from the process's
//! resolved [`TimingProfile`] (production: 3 s / 6 s / 10 s; TEST-ONLY compressed in debug builds
//! via `SENTIENT_TEST_FAST_TIMERS` — see that type's doc comment). This task is only the I/O
//! shell: it subscribes `tms/+/heartbeat` + `tms/+/device_info` over its own loopback `rumqttc`
//! client, drives the selector from inbound publishes plus a periodic re-evaluation tick at
//! `profile.eval_tick` (so the timers fire even with no traffic), and republishes the selected id
//! on the `watch` only when it changes. `CONFIG_ON_COMMS_LOSS` (the No-MC action) is a documented
//! no-op — no config subsystem.

use std::time::{Duration, Instant};

use bin_rpi_platform::TimingProfile;
use lib_mapping::schema::{DeviceRole, Identity};
use rumqttc::{AsyncClient, Event, Incoming, MqttOptions, QoS};
use tokio::sync::watch;
use tokio::time::MissedTickBehavior;

use crate::tasks::types::{DeviceInfoRoleView, McSelector};

/// MQTT keep-alive for this task's own client connection.
const KEEP_ALIVE_SECS: u64 = 30;

/// Extract the sender device id from a `tms/{id}/{suffix}` topic (exactly three segments, `tms`
/// prefix, non-empty id). Returns `None` for any other shape.
fn sender_id(topic: &str) -> Option<&str> {
    let mut it = topic.split('/');
    match (it.next(), it.next(), it.next(), it.next()) {
        (Some("tms"), Some(id), Some(_suffix), None) if !id.is_empty() => Some(id),
        _ => None,
    }
}

/// Decode an inbound `DeviceInfo` payload far enough to learn whether the sender is a microgrid
/// controller (`ROLE_MICROGRID_CONTROLLER`). `None` when the payload does not decode.
fn parse_is_mc(payload: &[u8]) -> Option<bool> {
    serde_json_core::from_slice::<DeviceInfoRoleView>(payload)
        .ok()
        .map(|(view, _)| matches!(view.role, DeviceRole::MicrogridController))
}

/// Send `sel` on the `watch` only if it differs from the current value (so consumers wake on real
/// selection changes only).
fn publish_if_changed(tx: &watch::Sender<Option<Identity>>, sel: Option<Identity>) {
    tx.send_if_modified(|cur| {
        if *cur != sel {
            *cur = sel;
            true
        } else {
            false
        }
    });
}

/// Run the shared MC-selection machine until shutdown, publishing the selected MC id on `mc_tx`.
/// `client_id_seed` (the process device id) makes this task's loopback client-id unique.
/// `profile` is the process's `TimingProfile`, resolved ONCE at startup by
/// `bin_rpi::runtime::supervise` (production values, or TEST-ONLY compressed values in a debug
/// build — see [`TimingProfile`]'s doc comment) and passed down here unchanged.
pub async fn run_mc_select(
    client_id_seed: String,
    broker_host: String,
    broker_port: u16,
    mc_tx: watch::Sender<Option<Identity>>,
    mut shutdown: watch::Receiver<bool>,
    profile: TimingProfile,
) {
    let mut opts = MqttOptions::new(
        format!("{client_id_seed}-mc-select"),
        broker_host,
        broker_port,
    );
    opts.set_keep_alive(Duration::from_secs(KEEP_ALIVE_SECS));
    let (client, mut eventloop) = AsyncClient::new(opts, 10);

    for filter in ["tms/+/heartbeat", "tms/+/device_info"] {
        if let Err(e) = client.subscribe(filter, QoS::AtLeastOnce).await {
            // Non-fatal: the eventloop reconnects and rumqttc replays subscriptions; log and keep
            // running so the periodic evaluation (and thus the No-MC path) still works.
            log::warn!("mc_select: subscribe to {filter} failed (will retry via reconnect): {e}");
        }
    }

    let mut selector = McSelector::with_profile(&profile);
    let mut eval = tokio::time::interval(profile.eval_tick);
    eval.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    break;
                }
            }
            _ = eval.tick() => {
                let sel = selector.evaluate(Instant::now());
                publish_if_changed(&mc_tx, sel);
            }
            event = eventloop.poll() => {
                match event {
                    Ok(Event::Incoming(Incoming::Publish(p))) => {
                        if let Some(id) = sender_id(&p.topic) {
                            let now = Instant::now();
                            if p.topic.ends_with("/heartbeat") {
                                selector.on_heartbeat(id, now);
                            } else if p.topic.ends_with("/device_info") {
                                if let Some(is_mc) = parse_is_mc(&p.payload) {
                                    selector.on_role(id, is_mc);
                                }
                            }
                            let sel = selector.evaluate(now);
                            publish_if_changed(&mc_tx, sel);
                        }
                    }
                    Ok(_) => {}
                    Err(e) => {
                        log::debug!("mc_select: mqtt eventloop error (will retry): {e}");
                        tokio::time::sleep(Duration::from_millis(20)).await;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::{parse_is_mc, sender_id};

    #[test]
    fn sender_id_extracts_the_middle_segment() {
        assert_eq!(sender_id("tms/MC-1/heartbeat"), Some("MC-1"));
        assert_eq!(sender_id("tms/METEOR-001/device_info"), Some("METEOR-001"));
        // Wrong shapes.
        assert_eq!(sender_id("tms/MC-1"), None);
        assert_eq!(sender_id("tms//heartbeat"), None);
        assert_eq!(sender_id("other/MC-1/heartbeat"), None);
        assert_eq!(sender_id("tms/MC-1/a/b"), None);
    }

    #[test]
    fn parse_is_mc_skips_unknown_device_info_fields() {
        // A DeviceInfo-shaped payload: `role` sits after `device_id` and before nested objects,
        // arrays, and a trailing field — serde-json-core must skip all the unknown ones.
        let mc = br#"{"device_id":"MC-1","role":"ROLE_MICROGRID_CONTROLLER",
            "product":{"nsn":"","manufacturer_name":"x","serial_number":"y"},
            "topics":{"published_optional_topics":["A","B"]},
            "power_device":null,"control_service":null}"#;
        assert_eq!(parse_is_mc(mc), Some(true));

        let stor = br#"{"device_id":"BATT-1","role":"ROLE_STORAGE",
            "product":{"nsn":""},"power_device":{"power_ports":[]}}"#;
        assert_eq!(parse_is_mc(stor), Some(false));

        assert_eq!(parse_is_mc(b"not json"), None);
    }
}

//! DDS-side TMS Request/Reply control path (#324, WP6) — the DDS transport counterpart of
//! `crate::tasks::reply_listener`, sharing the transport-neutral decoded-request → `Reply` core
//! (`crate::tasks::request_reply`). The ONLY thing specific to this transport (vs. the MQTT
//! listener's JSON `serde_json_core::from_slice`) is the XCDR2 decode step
//! (`lib_tms::cdr::decode::{decode_energy_start_stop_request, decode_power_switch_request}`).
//!
//! **Per identity (D13), own participant:** like `tasks::reply_listener` opens its own loopback
//! `rumqttc` client per identity, this task builds its OWN [`lib_dds::DdsParticipant`] per identity
//! (small — this repo runs at most a couple of identities per unit) rather than sharing one across
//! tasks, sidestepping any `Sync` requirement on the seam trait (it only requires `Send`).
//!
//! **Not live yet — confirmed, not assumed.** Neither `EnergyStartStopRequest` nor
//! `PowerSwitchRequest` has a DDS patch type today (`dust_dds_derive` 0.15.0's
//! `#[dust_dds(optional)]` does not accept the `Option<PowerSwitchCloseConditions>` both carry — see
//! `lib_dds::dustdds::types::patch`'s module doc comment). Both `subscribe` calls below return
//! `DdsError::UnsupportedTopic` today; this task logs once and idles (awaiting only `shutdown`) —
//! never a crash, never a busy-loop (the `tokio::select!` arms are gated `if energy_ok`/`if
//! switch_ok`, computed once, so a permanently-closed channel is never repolled). `Reply` DOES have
//! a patch type (it is optional-free), so the moment a follow-up lands the two request patch types, this
//! task starts answering with zero further code changes.

use lib_dds::{DdsConfig, DdsHandle, DdsSample};
use lib_mapping::schema::Identity;
use lib_tms::cdr::decode::{decode_energy_start_stop_request, decode_power_switch_request};
use lib_tms::generated_qos::qos_profile;
use lib_tms::TmsTopicId;
use tokio::sync::mpsc;
use tokio::sync::watch;

use super::util::render_and_send;
use crate::config::IdentityCfg;
use crate::tasks::request_reply;
use crate::tasks::util::bounded;

/// Bounded inbound-sample channel depth per named request topic — small; a burst beyond this is a
/// misbehaving/duplicating requester, not a normal operating condition.
const SAMPLE_QUEUE_DEPTH: usize = 16;

fn handle_energy(device_id: &Identity, handle: &DdsHandle, payload: &[u8]) {
    let reply = match decode_energy_start_stop_request(payload) {
        Ok(req) => request_reply::energy_start_stop_reply(device_id, &req),
        Err(e) => {
            log::debug!("dds request_listener: EnergyStartStopRequest decode failed: {e:?}");
            request_reply::bad_request(device_id.clone())
        }
    };
    render_and_send(handle, device_id, TmsTopicId::Reply, &reply);
}

fn handle_switch(device_id: &Identity, handle: &DdsHandle, payload: &[u8]) {
    let reply = match decode_power_switch_request(payload) {
        Ok(req) => request_reply::power_switch_reply(device_id, &req),
        Err(e) => {
            log::debug!("dds request_listener: PowerSwitchRequest decode failed: {e:?}");
            request_reply::bad_request(device_id.clone())
        }
    };
    render_and_send(handle, device_id, TmsTopicId::Reply, &reply);
}

/// Listen for inbound TMS requests over DDS and publish a `Reply` for ONE injected identity (D13).
/// `dds_config` builds this task's own participant (see the module doc comment); `handle` is the
/// SAME [`DdsHandle`] `DdsTap`/`tasks::dds::heartbeat`/`tasks::dds::state_publisher` enqueue on.
pub async fn run_dds_request_listener(
    identity: IdentityCfg,
    dds_config: DdsConfig,
    handle: DdsHandle,
    mut shutdown: watch::Receiver<bool>,
) {
    let device_id: Identity = bounded(&identity.device_id);

    let participant = match lib_dds::dustdds::build_participant(&dds_config) {
        Ok(p) => p,
        Err(e) => {
            log::warn!(
                "dds request_listener: participant build failed for {device_id}: {e}; disabled"
            );
            let _ = shutdown.changed().await;
            return;
        }
    };

    let (energy_tx, mut energy_rx) = mpsc::channel::<DdsSample>(SAMPLE_QUEUE_DEPTH);
    let (switch_tx, mut switch_rx) = mpsc::channel::<DdsSample>(SAMPLE_QUEUE_DEPTH);

    let energy_ok = participant
        .subscribe(
            TmsTopicId::EnergyStartStopRequest,
            "EnergyStartStopRequest",
            "EnergyStartStopRequest",
            qos_profile(TmsTopicId::EnergyStartStopRequest),
            energy_tx,
        )
        .is_ok();
    let switch_ok = participant
        .subscribe(
            TmsTopicId::PowerSwitchRequest,
            "PowerSwitchRequest",
            "PowerSwitchRequest",
            qos_profile(TmsTopicId::PowerSwitchRequest),
            switch_tx,
        )
        .is_ok();

    if !energy_ok && !switch_ok {
        log::info!(
            "dds request_listener: no request-type DDS patch yet for {device_id} (see \
             lib_dds::dustdds::types::patch's doc comment); idling until shutdown"
        );
    }

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    break;
                }
            }
            Some(sample) = energy_rx.recv(), if energy_ok => {
                handle_energy(&device_id, &handle, &sample.payload);
            }
            Some(sample) = switch_rx.recv(), if switch_ok => {
                handle_switch(&device_id, &handle, &sample.payload);
            }
        }
    }
}

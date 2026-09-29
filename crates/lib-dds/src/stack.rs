//! The DDS stack task: owns the participant and a lazy `(device_id, topic)` → writer map, draining
//! the outbound queue and publishing each pre-serialized XCDR2 payload. Behavior-only (no type
//! definitions) — programs against the [`DdsParticipant`]/[`DdsWriter`] seam, never the backend.

use std::collections::HashMap;

use lib_tms::generated_qos::qos_profile;
use lib_tms::{Identity, TmsTopicId};
use tokio::sync::mpsc;

use crate::types::{
    DdsConfig, DdsError, DdsHandle, DdsOutbound, DdsParticipant, DdsWriter, Result,
};

/// Create the bounded outbound channel feeding [`run_dds_stack`]. Clone the [`DdsHandle`] into every
/// tap; hand the receiver to `run_dds_stack`.
pub fn dds_channel(capacity: usize) -> (DdsHandle, mpsc::Receiver<DdsOutbound>) {
    let (tx, rx) = mpsc::channel(capacity.max(1));
    (DdsHandle::new(tx), rx)
}

/// Run the DDS stack until the outbound channel closes.
///
/// Builds the backend participant + its custom UDP/IPv6 transport from `config`, then drains `rx`:
/// each [`DdsOutbound`] lazily gets-or-creates a writer keyed by `(device_id, topic)` (with the
/// topic's Annex D.5 QoS profile and `USER_DATA` = `device_id`) and publishes the payload bytes.
///
/// A per-message failure (unmapped topic, transient write error) is logged and skipped — one bad
/// sample never tears down the stack. A participant/transport build failure returns `Err`.
pub async fn run_dds_stack(config: DdsConfig, rx: mpsc::Receiver<DdsOutbound>) -> Result<()> {
    let participant = new_participant(&config)?;
    log::info!(
        "dds: stack up on domain {} iface {} (SPDP {} port {})",
        config.domain_id,
        config.iface,
        config.mcast_group,
        config.spdp_port()
    );
    pump(participant, rx).await
}

/// The drain loop over the outbound queue, factored out of [`run_dds_stack`] so the lazy
/// writer-map behavior is testable against a mock [`DdsParticipant`] (no live DDS stack).
async fn pump(
    participant: Box<dyn DdsParticipant>,
    mut rx: mpsc::Receiver<DdsOutbound>,
) -> Result<()> {
    let mut writers: HashMap<(Identity, TmsTopicId), Box<dyn DdsWriter>> = HashMap::new();

    while let Some(out) = rx.recv().await {
        let key = (out.device_id.clone(), out.topic);
        let writer = match writers.get(&key) {
            Some(w) => w,
            None => match make_writer(participant.as_ref(), out.topic, &out.device_id) {
                Ok(w) => writers.entry(key.clone()).or_insert(w),
                Err(DdsError::UnsupportedTopic(t)) => {
                    log::debug!("dds: no writer mapping for {t:?} yet (WP6) — dropping sample");
                    continue;
                }
                Err(e) => {
                    log::warn!("dds: writer create failed for {:?}: {e}", out.topic);
                    continue;
                }
            },
        };
        if let Err(e) = writer.write(&out.device_id, &out.payload) {
            log::warn!("dds: write failed for {:?}: {e}", out.topic);
        }
    }

    log::info!("dds: outbound channel closed — stack stopping");
    Ok(())
}

/// Create a writer for `topic` with its D.5 profile + `USER_DATA` = `device_id`.
fn make_writer(
    participant: &dyn DdsParticipant,
    topic: TmsTopicId,
    device_id: &Identity,
) -> Result<Box<dyn DdsWriter>> {
    let name = topic.as_str();
    // TMS leaf types share their topic's CamelCase name (e.g. Heartbeat). WP6 can supply a distinct
    // DDS type name per topic if any diverge.
    participant.create_writer(topic, name, name, qos_profile(topic), device_id)
}

#[cfg(feature = "backend-dustdds")]
fn new_participant(config: &DdsConfig) -> Result<Box<dyn DdsParticipant>> {
    crate::dustdds::build_participant(config)
}

#[cfg(not(feature = "backend-dustdds"))]
fn new_participant(_config: &DdsConfig) -> Result<Box<dyn DdsParticipant>> {
    Err(DdsError::Participant(
        "no DDS backend feature enabled (build with `backend-dustdds`)".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use crate::types::{DdsSample, DdsWriter};
    use lib_tms::generated_qos::QosProfile;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tokio::sync::mpsc::Sender;

    /// A mock participant that "supports" only Heartbeat, counting writer creations + writes so the
    /// lazy `(device, topic)` writer map can be asserted without a live DDS stack.
    struct CountingParticipant {
        creates: Arc<AtomicUsize>,
        writes: Arc<AtomicUsize>,
    }
    struct CountingWriter {
        writes: Arc<AtomicUsize>,
    }
    impl DdsWriter for CountingWriter {
        fn write(&self, _key: &Identity, _payload: &[u8]) -> Result<()> {
            self.writes.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }
    impl DdsParticipant for CountingParticipant {
        fn create_writer(
            &self,
            topic: TmsTopicId,
            _topic_name: &str,
            _type_name: &str,
            _profile: QosProfile,
            _user_data: &Identity,
        ) -> Result<Box<dyn DdsWriter>> {
            if topic == TmsTopicId::Heartbeat {
                self.creates.fetch_add(1, Ordering::SeqCst);
                Ok(Box::new(CountingWriter {
                    writes: self.writes.clone(),
                }))
            } else {
                Err(DdsError::UnsupportedTopic(topic))
            }
        }
        fn subscribe(
            &self,
            _topic: TmsTopicId,
            _topic_name: &str,
            _type_name: &str,
            _profile: QosProfile,
            _tx: Sender<DdsSample>,
        ) -> Result<()> {
            Ok(())
        }
    }

    fn ident(s: &str) -> Identity {
        let mut i = Identity::new();
        i.push_str(s).unwrap();
        i
    }

    fn hb(dev: &str, seq: u8) -> DdsOutbound {
        DdsOutbound {
            device_id: ident(dev),
            topic: TmsTopicId::Heartbeat,
            payload: vec![seq],
        }
    }

    #[tokio::test]
    async fn writer_map_is_lazy_and_per_key() {
        let creates = Arc::new(AtomicUsize::new(0));
        let writes = Arc::new(AtomicUsize::new(0));
        let participant = Box::new(CountingParticipant {
            creates: creates.clone(),
            writes: writes.clone(),
        });

        let (tx, rx) = mpsc::channel(16);
        for seq in 0..3 {
            tx.send(hb("METEOR-001", seq)).await.unwrap();
        }
        for seq in 0..2 {
            tx.send(hb("METEOR-002", seq)).await.unwrap();
        }
        // An unmapped topic: dropped (no create, no write) — never tears down the pump.
        tx.send(DdsOutbound {
            device_id: ident("METEOR-001"),
            topic: TmsTopicId::ClockState,
            payload: vec![9],
        })
        .await
        .unwrap();
        drop(tx);

        pump(participant, rx).await.unwrap();

        // Two distinct (device, Heartbeat) keys → exactly two lazy writer creations (deduped).
        assert_eq!(
            creates.load(Ordering::SeqCst),
            2,
            "one writer per (device, topic), created on first use"
        );
        // Five Heartbeat writes; the unmapped ClockState produced neither a create nor a write.
        assert_eq!(writes.load(Ordering::SeqCst), 5);
    }
}

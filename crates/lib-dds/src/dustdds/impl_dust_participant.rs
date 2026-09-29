use core::net::Ipv6Addr;
use std::time::Duration;

use dust_dds::dds_async::domain_participant_factory::DomainParticipantFactoryAsync;
use dust_dds::domain::domain_participant_factory::DomainParticipantFactory;
use dust_dds::infrastructure::qos::DataWriterQos;
use dust_dds::infrastructure::sample_info::{ANY_INSTANCE_STATE, ANY_SAMPLE_STATE, ANY_VIEW_STATE};
use dust_dds::std_runtime::executor::Executor;
use dust_dds::std_runtime::timer::TimerDriver;
use dust_dds::std_runtime::StdRuntime;
use dust_dds::topic_definition::topic_description::TopicDescription;
use dust_dds::xtypes::serializer::Cdr2LeSerializer;
use tokio::sync::mpsc::Sender;

use super::prelude::*;
use super::qos_map;
use super::types::patch;
use crate::types::DdsConfig;

impl DustParticipant {
    /// Build the participant over the custom UDP/IPv6 transport for `config`.
    ///
    /// The async factory is `Box::leak`ed to a `&'static` (it hosts the participant-factory actor for
    /// the life of the process — one DDS stack per binary; the leak is a deliberate singleton, not a
    /// per-call allocation).
    pub(crate) fn build(config: &DdsConfig) -> Result<Self> {
        let scope = read_ifindex(&config.iface)?;
        let factory = Ipv6TransportFactory {
            scope,
            group: config.mcast_group,
            unicast_addr: config.unicast_addr.unwrap_or(Ipv6Addr::LOCALHOST),
            spdp_port: config.spdp_port(),
            fragment_size: 1344,
        };

        let runtime = StdRuntime::new(Executor::new(), TimerDriver::new());
        let app_id = std::process::id().to_ne_bytes();
        let host_id = [127, 0, 0, 1];
        let async_factory: &'static DomainParticipantFactoryAsync<StdRuntime> =
            Box::leak(Box::new(DomainParticipantFactoryAsync::new(
                runtime, app_id, host_id, factory,
            )));
        let dpf = DomainParticipantFactory::new(async_factory);

        let participant = dpf
            .create_participant(
                config.domain_id as i32,
                QosKind::Default,
                NO_LISTENER,
                NO_STATUS,
            )
            .map_err(|e| DdsError::Participant(format!("create_participant: {e:?}")))?;
        let publisher = participant
            .create_publisher(QosKind::Default, NO_LISTENER, NO_STATUS)
            .map_err(|e| DdsError::Participant(format!("create_publisher: {e:?}")))?;
        let subscriber = participant
            .create_subscriber(QosKind::Default, NO_LISTENER, NO_STATUS)
            .map_err(|e| DdsError::Participant(format!("create_subscriber: {e:?}")))?;

        Ok(Self {
            participant,
            publisher,
            subscriber,
        })
    }

    /// Get-or-create the DDS topic for type `M`. A topic name may be registered only once per
    /// participant, so a writer and a reader for the same topic must share it (create first, look up
    /// after) — dust_dds returns the same `TopicDescription` either way.
    fn topic_of<M: TypeSupport>(
        &self,
        topic: TmsTopicId,
        topic_name: &str,
        type_name: &str,
    ) -> Result<TopicDescription> {
        match self.participant.create_topic::<M>(
            topic_name,
            type_name,
            QosKind::Default,
            NO_LISTENER,
            NO_STATUS,
        ) {
            Ok(t) => Ok(t),
            Err(_) => self
                .participant
                .lookup_topicdescription(topic_name)
                .ok()
                .flatten()
                .ok_or_else(|| DdsError::WriterCreate {
                    topic,
                    detail: format!("topic `{topic_name}` neither creatable nor found"),
                }),
        }
    }

    /// Create a typed `DataWriter<M>` for the topic + wrap it as an erased [`DustWriter`].
    fn typed_writer<M: TypeSupport + Send + 'static>(
        &self,
        topic: TmsTopicId,
        topic_name: &str,
        type_name: &str,
        qos: DataWriterQos,
    ) -> Result<Box<dyn DdsWriter>> {
        let dds_topic = self.topic_of::<M>(topic, topic_name, type_name)?;
        let writer = self
            .publisher
            .create_datawriter::<M>(&dds_topic, QosKind::Specific(qos), NO_LISTENER, NO_STATUS)
            .map_err(|e| DdsError::WriterCreate {
                topic,
                detail: format!("create_datawriter: {e:?}"),
            })?;
        Ok(Box::new(DustWriter { writer }))
    }

    /// Create a typed `DataReader<M>` and spawn a poll loop re-serializing received samples to XCDR2
    /// bytes and forwarding them as [`DdsSample`]s (the request→Reply listener path is fully wired in
    /// WP6). The loop runs on a dedicated OS thread — dust_dds's sync `take()` blocks on its own
    /// executor, which must not be nested inside a tokio worker — forwarding via `blocking_send`.
    fn typed_reader<M: TypeSupport + Clone + Send + 'static>(
        &self,
        topic: TmsTopicId,
        topic_name: &str,
        type_name: &str,
        profile: QosProfile,
        tx: Sender<DdsSample>,
    ) -> Result<()> {
        let dds_topic = self.topic_of::<M>(topic, topic_name, type_name)?;
        let reader = self
            .subscriber
            .create_datareader::<M>(
                &dds_topic,
                QosKind::Specific(qos_map::to_reader_qos(profile)?),
                NO_LISTENER,
                NO_STATUS,
            )
            .map_err(|e| DdsError::WriterCreate {
                topic,
                detail: format!("create_datareader: {e:?}"),
            })?;
        let spawned = std::thread::Builder::new()
            .name(format!("dds-sub-{}", topic_name))
            .spawn(move || loop {
                if let Ok(samples) = reader.take(
                    i32::MAX,
                    ANY_SAMPLE_STATE,
                    ANY_VIEW_STATE,
                    ANY_INSTANCE_STATE,
                ) {
                    for s in samples {
                        if let Some(data) = s.data {
                            let dynamic = data.create_dynamic_sample();
                            if let Ok(payload) = Cdr2LeSerializer::serialize(&dynamic) {
                                if tx.blocking_send(DdsSample { topic, payload }).is_err() {
                                    return; // receiver gone
                                }
                            }
                        }
                    }
                }
                std::thread::sleep(Duration::from_millis(50));
            });
        if let Err(e) = spawned {
            return Err(DdsError::Participant(format!(
                "spawn subscriber thread for {topic:?}: {e}"
            )));
        }
        Ok(())
    }
}

impl DdsParticipant for DustParticipant {
    fn create_writer(
        &self,
        topic: TmsTopicId,
        topic_name: &str,
        type_name: &str,
        profile: QosProfile,
        user_data: &Identity,
    ) -> Result<Box<dyn DdsWriter>> {
        let qos = qos_map::to_writer_qos(profile, user_data)?;
        match topic {
            TmsTopicId::Heartbeat => {
                self.typed_writer::<patch::Heartbeat>(topic, topic_name, type_name, qos)
            }
            // WP6 (#324): the `DdsTap` telemetry topic the runtime smoke test proves end to end.
            TmsTopicId::DcMeasurementUpdate => {
                self.typed_writer::<patch::DcMeasurementUpdate>(topic, topic_name, type_name, qos)
            }
            // WP6 (#324): the request-reply response leg (`tasks::dds::request_listener`).
            TmsTopicId::Reply => {
                self.typed_writer::<patch::Reply>(topic, topic_name, type_name, qos)
            }
            // WP6.5: the ONLY one of the eight target writer-map topics that is `@optional`-free
            // across its whole closure, so it mirrors via `dust_dds_derive` today.
            TmsTopicId::DcLoadSharingState => {
                self.typed_writer::<patch::DcLoadSharingState>(topic, topic_name, type_name, qos)
            }
            // #332: the seven remaining WP6.5 topics are `@optional`-bearing and are now UNBLOCKED
            // by the vendored `dust_dds`/`dust_dds_derive` `@optional` overhaul — the derive keys on
            // `field_attributes.optional` (mapping against inner `T`), and the FINAL/APPENDABLE
            // XCDR2 path emits a 1-byte `<is_present>` boolean per declared optional (byte-identical
            // to `lib_tms::cdr::emit_optional`). See `dustdds::types::patch`'s module doc and
            // `vendor/dust_dds/PATCHES.md`.
            TmsTopicId::DeviceInfo => {
                self.typed_writer::<patch::DeviceInfo>(topic, topic_name, type_name, qos)
            }
            TmsTopicId::PowerPortState => {
                self.typed_writer::<patch::PowerPortState>(topic, topic_name, type_name, qos)
            }
            TmsTopicId::EnergyStartStopState => {
                self.typed_writer::<patch::EnergyStartStopState>(topic, topic_name, type_name, qos)
            }
            TmsTopicId::ActiveMicrogridControllerState => self
                .typed_writer::<patch::ActiveMicrogridControllerState>(
                    topic, topic_name, type_name, qos,
                ),
            TmsTopicId::PowerHardwareUpdate => {
                self.typed_writer::<patch::PowerHardwareUpdate>(topic, topic_name, type_name, qos)
            }
            TmsTopicId::StorageUpdate => {
                self.typed_writer::<patch::StorageUpdate>(topic, topic_name, type_name, qos)
            }
            TmsTopicId::ActiveDiagnosticState => {
                self.typed_writer::<patch::ActiveDiagnosticState>(topic, topic_name, type_name, qos)
            }
            other => Err(DdsError::UnsupportedTopic(other)),
        }
    }

    fn subscribe(
        &self,
        topic: TmsTopicId,
        topic_name: &str,
        type_name: &str,
        profile: QosProfile,
        tx: Sender<DdsSample>,
    ) -> Result<()> {
        match topic {
            TmsTopicId::Heartbeat => {
                self.typed_reader::<patch::Heartbeat>(topic, topic_name, type_name, profile, tx)
            }
            // Reader-side counterparts of `create_writer`'s three other optional-free patch types
            // (#325, WP7): the SAME `patch::X: TypeSupport + Clone` impls `create_writer` already
            // uses, just not previously wired through `typed_reader` — nothing in `bin_rpi`'s own
            // tasks calls `subscribe` for these (the request/reply listener is the only production
            // subscribe call site, and its two topics remain `@optional`-blocked below), so this gap
            // went unnoticed until WP7's e2e test needed a real subscriber for the mapped topics.
            TmsTopicId::DcMeasurementUpdate => self.typed_reader::<patch::DcMeasurementUpdate>(
                topic, topic_name, type_name, profile, tx,
            ),
            TmsTopicId::DcLoadSharingState => self.typed_reader::<patch::DcLoadSharingState>(
                topic, topic_name, type_name, profile, tx,
            ),
            TmsTopicId::Reply => {
                self.typed_reader::<patch::Reply>(topic, topic_name, type_name, profile, tx)
            }
            // #332: reader-side counterparts of the seven `@optional`-bearing writer-map topics,
            // now unblocked by the vendored `@optional` overhaul (the deserializer mirrors the
            // 1-byte `<is_present>` boolean via an overridable presence-consume). Same `patch::X:
            // TypeSupport + Clone` impls `create_writer` uses.
            TmsTopicId::DeviceInfo => {
                self.typed_reader::<patch::DeviceInfo>(topic, topic_name, type_name, profile, tx)
            }
            TmsTopicId::PowerPortState => self
                .typed_reader::<patch::PowerPortState>(topic, topic_name, type_name, profile, tx),
            TmsTopicId::EnergyStartStopState => self.typed_reader::<patch::EnergyStartStopState>(
                topic, topic_name, type_name, profile, tx,
            ),
            TmsTopicId::ActiveMicrogridControllerState => self
                .typed_reader::<patch::ActiveMicrogridControllerState>(
                    topic, topic_name, type_name, profile, tx,
                ),
            TmsTopicId::PowerHardwareUpdate => self.typed_reader::<patch::PowerHardwareUpdate>(
                topic, topic_name, type_name, profile, tx,
            ),
            TmsTopicId::StorageUpdate => {
                self.typed_reader::<patch::StorageUpdate>(topic, topic_name, type_name, profile, tx)
            }
            TmsTopicId::ActiveDiagnosticState => self.typed_reader::<patch::ActiveDiagnosticState>(
                topic, topic_name, type_name, profile, tx,
            ),
            // `EnergyStartStopRequest`/`PowerSwitchRequest` (the two named request topics
            // `tasks::dds::request_listener` wants to answer) are still not mirrored — no patch
            // mirror type exists for them yet — so they fall through to `UnsupportedTopic` below;
            // `request_listener` logs once and idles rather than crashing.
            other => Err(DdsError::UnsupportedTopic(other)),
        }
    }
}

/// Read an interface's if-index (multicast scope) from `/sys/class/net/<iface>/ifindex`.
fn read_ifindex(iface: &str) -> Result<u32> {
    if iface.is_empty() {
        return Err(DdsError::Participant("empty interface name".to_string()));
    }
    let path = format!("/sys/class/net/{iface}/ifindex");
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
        .ok_or_else(|| {
            DdsError::Participant(format!("cannot read if-index for interface `{iface}`"))
        })
}

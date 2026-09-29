use dust_dds::domain::domain_participant::DomainParticipant;
use dust_dds::publication::publisher::Publisher;
use dust_dds::subscription::subscriber::Subscriber;

/// The dust_dds-backed [`crate::DdsParticipant`]: an owning handle to the domain participant (created
/// over the custom UDP/IPv6 transport) plus its default publisher/subscriber. Impl (build +
/// `DdsParticipant`) in `impl_dust_participant.rs`.
pub struct DustParticipant {
    pub(crate) participant: DomainParticipant,
    pub(crate) publisher: Publisher,
    pub(crate) subscriber: Subscriber,
}

use super::dds_sample::DdsSample;
use super::writer::DdsWriter;
use super::Result;
use lib_tms::generated_qos::QosProfile;
use lib_tms::{Identity, TmsTopicId};
use tokio::sync::mpsc::Sender;

/// The owned DDS participant seam: creates per-topic writers/readers with the topic's D.5 QoS +
/// per-writer `USER_DATA` = deviceId. One backend adapter implements this (`dustdds::DustParticipant`).
///
/// `create_writer` maps a `(topic, profile, user_data)` triple to a backend `DataWriter`; the adapter
/// translates `profile` → the backend's QoS types, returning [`super::DdsError::UnsupportedQos`]
/// rather than downgrading. `subscribe` registers a reader that forwards received samples to `tx`.
pub trait DdsParticipant: Send {
    /// Create a writer for `topic` (bare CamelCase `topic_name`, DDS `type_name`) under `profile`,
    /// with `USER_DATA` = `user_data` (the device identity).
    fn create_writer(
        &self,
        topic: TmsTopicId,
        topic_name: &str,
        type_name: &str,
        profile: QosProfile,
        user_data: &Identity,
    ) -> Result<Box<dyn DdsWriter>>;

    /// Subscribe to `topic`, forwarding each received [`DdsSample`] to `tx`. (Reader wiring for the
    /// request→Reply path is completed with the Pi taps in WP6.)
    fn subscribe(
        &self,
        topic: TmsTopicId,
        topic_name: &str,
        type_name: &str,
        profile: QosProfile,
        tx: Sender<DdsSample>,
    ) -> Result<()>;
}

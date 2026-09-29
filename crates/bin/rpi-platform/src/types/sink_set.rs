use super::host_tap::HostTap;

/// The set of configured secondary-publish taps offered every [`super::tap_event::TapEvent`]
/// alongside the primary MQTT publish (see that type's doc comment).
#[derive(Clone, Default)]
pub struct SinkSet {
    pub taps: Vec<HostTap>,
}

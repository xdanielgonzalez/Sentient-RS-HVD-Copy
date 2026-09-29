use lib_net::channel::{MqttUnit, Sender};

/// A [`lib_mapping::sink::Publisher`] that renders each publish into an [`MqttUnit`] boundary
/// message and hands it to a `channel::Sender` — the MCU forward path onto the publish stage (a
/// concrete `embassy_sync` channel half on `bin/mcu-rp2350`, via the `channel-embassy` feature; the
/// sender type is generic here, so this crate never names `embassy-sync` itself).
///
/// `qos` is fixed at 1 (at-least-once) — the same default the render/publish call sites elsewhere
/// in the workspace assume. `healthy()` reflects only the LAST `try_send` outcome (see
/// [`lib_mapping::sink::Publisher::healthy`]'s contract); a bounds overflow building the
/// [`MqttUnit`] (topic/payload too big) fails closed with a `PublishError` WITHOUT attempting a
/// send, so it does not by itself change `healthy()`.
pub struct ChannelPublisher<S: Sender<Item = MqttUnit>> {
    pub(in super::super) sender: S,
    pub(in super::super) last_ok: bool,
}

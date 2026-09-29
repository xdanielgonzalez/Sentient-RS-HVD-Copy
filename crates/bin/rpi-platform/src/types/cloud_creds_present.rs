/// Fails if `CLOUD_MQTT_PASS` or a required non-secret `[cloud]` field is missing. A device with
/// no `[cloud]` section at all (local-only) passes trivially — see [`super::CloudCheckInfo`]'s doc,
/// which mirrors `mosquitto_bridge::render_bridge`'s fail-closed conditions.
pub(in super::super) struct CloudCredsPresent;

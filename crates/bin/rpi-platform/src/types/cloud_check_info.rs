/// Non-secret `[cloud]` bridge fields the host preflight checks need. Mirrors the fail-closed
/// conditions in `bin-rpi`'s `mosquitto_bridge::render_bridge` (host/username/ca_file
/// non-empty, `CLOUD_MQTT_PASS` non-empty) without this crate depending on that binary's private
/// `Config`/`CloudConfig` types — the caller (`bin-rpi`) computes these from its own config.
#[derive(Debug, Clone, Copy)]
pub struct CloudCheckInfo {
    pub host_present: bool,
    pub username_present: bool,
    pub ca_file_present: bool,
    pub password_present: bool,
}

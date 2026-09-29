use serde::Deserialize;

/// MQTT 3.1.1 §4.3 defines exactly three QoS levels (0/1/2) — see [`CloudConfig::validate`].
pub const MAX_MQTT_QOS: u8 = 2;

/// Non-secret private-cloud bridge settings. Everything Sentient (#18) provides except the
/// password lands here; the password lands in the `CLOUD_MQTT_PASS` secret env file instead.
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CloudConfig {
    /// Private-cloud MQTT broker hostname/IP.
    pub host: String,
    /// Private-cloud MQTT broker port (TLS default). Must be non-zero — see
    /// [`CloudConfig::validate`].
    #[serde(default = "default_cloud_port")]
    pub port: u16,
    /// Bridge username (the credential Sentient provisions for this device).
    pub username: String,
    /// Path to the Sentient CA certificate used to validate the cloud endpoint's TLS cert.
    pub ca_file: String,
    /// Optional explicit MQTT client id for the bridge connection to the remote broker.
    #[serde(default)]
    pub remote_clientid: Option<String>,
    /// Topic filter bridged upstream (`out`).
    #[serde(default = "default_cloud_topic")]
    pub topic: String,
    /// QoS for the bridged topic. Must be 0, 1, or 2 — see [`CloudConfig::validate`].
    #[serde(default = "default_cloud_qos")]
    pub qos: u8,
}

fn default_cloud_port() -> u16 {
    8883
}
fn default_cloud_topic() -> String {
    "tms/#".into()
}
fn default_cloud_qos() -> u8 {
    1
}

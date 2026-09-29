use serde::Deserialize;

// `MAX_MQTT_QOS` (MQTT 3.1.1 §4.3's 0/1/2 bound, used by `GreengrassConfig::validate` in
// `impl_greengrass_config.rs`) lives once, on `CloudConfig` — see `cloud_config.rs` — reached via
// `crate::config::MAX_MQTT_QOS`/the module `prelude`'s `types::*` re-export, not redefined here, so
// the two mTLS-bridge configs share exactly one QoS bound rather than two copies that could drift.

/// Greengrass backwards-compat bridge config (Scope D, closes #52-#54) — see
/// `docs/gap-analysis-meteor-parity.md`'s "Greengrass local IPC publish" finding. Absent entirely
/// on a device that never bridges to the legacy AWS Greengrass pipeline (no `[greengrass]`
/// section at all — same "opt-in secondary publish target" shape as `[disk]`/`[meshtastic]`).
///
/// This is the **mosquitto-mTLS-bridge** route (mosquitto Bridge 3, rendered by
/// `crate::mosquitto_bridge::render_greengrass_bridge` from this config, out to the AWS
/// Greengrass core's local MQTT broker), NOT the native `awsiot.greengrasscoreipc` local-publish
/// API — no Rust equivalent of that IPC route exists (see the gap-analysis finding above);
/// `component_name` reserves that unimplemented surface for a possible future work stream.
///
/// PROVISIONAL: the mTLS field set (host/port/cert_file/key_file/ca_file) mirrors `CloudConfig`'s
/// shape (the same mosquitto-bridge pattern), and `remote_topic_prefix`'s exact AWS-side topic
/// scheme is a best-effort guess — no reference `example/aws` Greengrass bridge config was
/// available in this environment (see CLAUDE.md's reference-prototype policy). Confirm against a
/// real AWS IoT Greengrass core's expected inbound topic namespace before this is load-bearing.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GreengrassConfig {
    /// The AWS IoT Greengrass core's local MQTT broker hostname/IP (the mosquitto bridge's
    /// `address`).
    pub host: String,
    /// Greengrass core broker port (TLS default — the Greengrass core's local broker listens with
    /// its own device-certificate-issued server identity, same as the private-cloud bridge). Must
    /// be non-zero — see [`GreengrassConfig::validate`].
    #[serde(default = "default_greengrass_port")]
    pub port: u16,
    /// This device's X.509 client certificate (mTLS — Greengrass core auth is certificate-based,
    /// unlike the private-cloud bridge's username/password).
    pub cert_file: String,
    /// This device's X.509 private key, matching `cert_file`.
    pub key_file: String,
    /// CA certificate used to validate the Greengrass core's TLS identity.
    pub ca_file: String,
    /// Optional explicit MQTT client id for the bridge connection to the Greengrass core.
    #[serde(default)]
    pub remote_clientid: Option<String>,
    /// Local topic pattern bridged out (mirrors `GreengrassTap`'s
    /// `telemetry/{device_type}/{device_id}` publish topics).
    #[serde(default = "default_greengrass_local_topic")]
    pub local_topic: String,
    /// Remote-side topic prefix the local topic is remapped under (mosquitto bridge topic
    /// remapping — see `render_greengrass_bridge`'s doc comment). PROVISIONAL: the real AWS
    /// Greengrass core topic namespace this should land under is unconfirmed.
    #[serde(default)]
    pub remote_topic_prefix: String,
    /// QoS for the bridged topic. Must be 0, 1, or 2 — see [`GreengrassConfig::validate`].
    #[serde(default = "default_greengrass_qos")]
    pub qos: u8,
    /// The Greengrass component name this device would publish local IPC messages as, if/when the
    /// native `awsiot.greengrasscoreipc` route (distinct from this mTLS mosquitto bridge) is ever
    /// implemented. Unused by `render_greengrass_bridge` — reserved surface, `#[allow(dead_code)]`
    /// marks that as intentional rather than an oversight.
    #[serde(default)]
    #[allow(dead_code)]
    pub component_name: String,
}

fn default_greengrass_port() -> u16 {
    8883
}
fn default_greengrass_local_topic() -> String {
    "telemetry/#".into()
}
fn default_greengrass_qos() -> u8 {
    1
}

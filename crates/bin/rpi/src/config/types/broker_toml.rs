use serde::Deserialize;

/// The embedded MQTT broker's bind host/port. `port` must be non-zero — see
/// [`BrokerToml::validate`].
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrokerToml {
    pub host: String,
    pub port: u16,
}

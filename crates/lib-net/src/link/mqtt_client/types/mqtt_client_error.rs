use crate::link::mqtt_wire::WireError;

/// MQTT control-packet encode/parse failures. Fail-closed: any error means the packet is not
/// emitted / not trusted. A thin re-export of the shared `WireError` (kept as its own name here
/// since it predates the `crate::link::mqtt_wire` extraction and is part of this module's public API).
pub type MqttClientError = WireError;

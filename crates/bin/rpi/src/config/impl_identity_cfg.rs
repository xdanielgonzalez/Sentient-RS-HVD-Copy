use super::prelude::*;

impl IdentityCfg {
    /// The legacy `telemetry/{device_type}/{device_id}` topic segment (and payload `device_type`
    /// field) for this identity: `"battery"` for a Seplos bus, `"inverter"` for a Voltronic bus.
    pub fn device_type(&self) -> &'static str {
        match self.protocol {
            ProtocolCfg::Seplos => "battery",
            ProtocolCfg::Voltronic => "inverter",
        }
    }
}

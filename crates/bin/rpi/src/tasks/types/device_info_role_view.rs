use lib_mapping::schema::DeviceRole;
use serde::Deserialize;

/// Minimal projection of an inbound `DeviceInfo` payload — just the `role` field the MC-selection
/// machine needs. `serde-json-core` skips the other (unknown) `DeviceInfo` fields, so this decodes
/// straight from the full wire `DeviceInfo` JSON without materializing the whole struct.
#[derive(Debug, Deserialize)]
pub struct DeviceInfoRoleView {
    pub role: DeviceRole,
}

use super::prelude::*;

impl ProductParams {
    /// Snapshot the shared `ProductInfo` fields out of `[device]` for injection into the
    /// per-identity heartbeat tasks (D13) — an owned copy, so the tasks never re-load `Config`.
    pub fn from_device(device: &DeviceConfig) -> Self {
        ProductParams {
            manufacturer: device.manufacturer.clone(),
            model: device.model.clone(),
            serial_number: device.serial_number.clone(),
            firmware_version: device.firmware_version.clone(),
        }
    }
}

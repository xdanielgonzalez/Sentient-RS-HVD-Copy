use super::cloud_check_info::CloudCheckInfo;

/// Non-secret inputs the host's preflight checks need, gathered by the caller from `Config` (this
/// crate cannot see `bin-rpi::config::Config` directly).
#[derive(Debug, Clone)]
pub struct PreflightInputs {
    /// `device.id` is non-empty.
    pub device_id_present: bool,
    /// `poll.dip_addresses` is non-empty.
    pub dip_addresses_present: bool,
    /// `[cloud]` bridge config, if this device pushes to the cloud at all.
    pub cloud: Option<CloudCheckInfo>,
}

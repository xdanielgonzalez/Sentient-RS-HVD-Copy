use super::cloud_check_info::CloudCheckInfo;

/// Preflight context for [`super::host_platform::HostPlatform`]: what `SerialPresent`,
/// `ConfigValid`, and the host-specific cloud checks inspect.
pub struct HostCtx {
    pub(in super::super) serial_present: bool,
    pub(in super::super) serial_label: String,
    pub(in super::super) config_valid: Result<(), &'static str>,
    pub(in super::super) cloud: Option<CloudCheckInfo>,
}

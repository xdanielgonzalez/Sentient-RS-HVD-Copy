use super::broker_toml::BrokerToml;
use super::bus_config::BusConfig;
use super::capture_config::CaptureConfig;
use super::cloud_config::CloudConfig;
use super::dds_toml::DdsToml;
use super::device_config::DeviceConfig;
use super::direction_cfg::DirectionCfg;
use super::disk_config::DiskConfig;
use super::greengrass_config::GreengrassConfig;
use super::location_config::LocationConfig;
use super::meshtastic_config::MeshtasticConfig;
use super::poll_config::PollConfig;
use super::publish_config::PublishConfig;
use super::serial_config::SerialConfig;
use serde::Deserialize;

/// Top-level configuration. `deny_unknown_fields` (and on every nested section struct) so a typo'd
/// or stale config key fails loudly at startup instead of being silently ignored (see
/// `docs/gap-analysis-meteor-parity.md`'s "Config-level partials" finding) — `Config::load`'s
/// `Env::prefixed("SENTIENT_").ignore(&["config", "test_fast_timers"])` keeps this compatible with
/// two non-TOML `SENTIENT_`-prefixed env vars so neither is rejected as an unknown key by the same
/// tightening: `SENTIENT_CONFIG` (the config-file-path override read directly in `main.rs`) and
/// `SENTIENT_TEST_FAST_TIMERS` (the TEST-ONLY timer-compression switch
/// `bin_rpi_platform::TimingProfile::resolve` reads directly, debug builds only).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub device: DeviceConfig,
    /// The legacy single-port `[serial]` section. Mutually exclusive with `[[bus]]` — see
    /// [`Config::buses`]'s doc comment. `Option` (not required) so a `[[bus]]`-only config is
    /// valid.
    #[serde(default)]
    pub serial: Option<SerialConfig>,
    /// Poll cadence for the legacy single-port `[serial]` section (ignored when `[[bus]]` is used
    /// — each bus carries its own `poll`).
    #[serde(default)]
    pub poll: PollConfig,
    #[serde(default)]
    pub broker: BrokerToml,
    #[serde(default)]
    pub publish: PublishConfig,
    /// Private-cloud MQTT bridge (mosquitto `connection cloud-relay`) — the non-secret half of the
    /// contract. Absent entirely on a local-only device (no cloud push configured yet). The
    /// password is deliberately NOT a field here: it comes only from the `CLOUD_MQTT_PASS` secret
    /// env var, read at render time (see `crate::mosquitto_bridge`), never from TOML.
    #[serde(default)]
    pub cloud: Option<CloudConfig>,
    /// Multi-port `[[bus]]` array-of-tables. Mutually exclusive with the legacy `[serial]` — see
    /// [`Config::buses`]'s doc comment.
    #[serde(default)]
    pub bus: Vec<BusConfig>,
    /// Disk / JSON-lines telemetry logging (Scope A). STUB — see `disk_config.rs`.
    #[serde(default)]
    pub disk: Option<DiskConfig>,
    /// Greengrass backwards-compat bridge (Scope D). STUB — see `greengrass_config.rs`.
    #[serde(default)]
    pub greengrass: Option<GreengrassConfig>,
    /// Meshtastic mesh-radio secondary publish target. STUB — see `meshtastic_config.rs`.
    #[serde(default)]
    pub meshtastic: Option<MeshtasticConfig>,
    /// Multi-tier location tagging. STUB — see `location_config.rs`.
    #[serde(default)]
    pub location: Option<LocationConfig>,
    /// RS-485 driver-enable mechanism. STUB — see `direction_cfg.rs`.
    #[serde(default)]
    pub direction: Option<DirectionCfg>,
    /// Raw pre-parse wire capture-to-disk (decision 0001's open risk — field-collect real
    /// Voltronic/Seplos wire captures). See `capture_config.rs`; wired end to end by
    /// `runtime.rs`'s `supervise` via the `bin_rpi_platform::CaptureSerial` transport decorator.
    #[serde(default)]
    pub capture: Option<CaptureConfig>,
    /// DDS/RTPS (MIL-STD-3071) transport (#324, WP6). ABSENT means DDS is off entirely — MQTT
    /// stays the only transport, byte-for-byte the pre-#324 behavior. See `dds_toml.rs`; wired end
    /// to end by `runtime.rs`'s `supervise` (participant + `run_dds_stack` + `DdsTap` + the
    /// `tasks::dds::*` background tasks), all conditional on this field.
    #[serde(default)]
    pub dds: Option<DdsToml>,
}

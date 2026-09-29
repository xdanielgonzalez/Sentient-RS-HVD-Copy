use super::prelude::*;
use lib_mapping::schema::DeviceRole;

/// Truncate `s` to the first 32 chars on a UTF-8 boundary — the `Identity` (`String<32>`) bound
/// every derived `device_id` must fit (D13).
fn truncate_identity(s: &str) -> String {
    let end = s.char_indices().nth(32).map_or(s.len(), |(i, _)| i);
    s[..end].to_string()
}

impl Config {
    /// Load from an optional TOML file, then overlay `SENTIENT_`-prefixed env vars (nested with
    /// `__`, e.g. `SENTIENT_SERIAL__PORT=/dev/ttyUSB0`).
    ///
    /// `.ignore(&["config", "test_fast_timers"])` excludes two non-TOML `SENTIENT_`-prefixed vars
    /// from the env provider — without this, `Config`'s `deny_unknown_fields` (see
    /// `types/config.rs`) would reject either the moment it's set:
    /// * `SENTIENT_CONFIG` — the config-file-path override `main.rs` reads directly via
    ///   `std::env::var`, never a TOML key.
    /// * `SENTIENT_TEST_FAST_TIMERS` — the TEST-ONLY timer-compression switch
    ///   `bin_rpi_platform::TimingProfile::resolve` reads directly via `std::env::var_os` (debug
    ///   builds only), also never a TOML key. See that type's doc comment for the full contract.
    pub fn load(toml_path: Option<&str>) -> anyhow::Result<Config> {
        use figment::providers::{Env, Format, Toml};
        use figment::Figment;
        let mut fig = Figment::new();
        if let Some(path) = toml_path {
            fig = fig.merge(Toml::file(path));
        }
        fig = fig.merge(
            Env::prefixed("SENTIENT_")
                .ignore(&["config", "test_fast_timers"])
                .split("__"),
        );
        Ok(fig.extract()?)
    }

    /// Normalize the configured serial bus(es) into one canonical `Vec<BusConfig>`, regardless of
    /// which config shape was used:
    ///
    /// * `[[bus]]` present (non-empty) and `[serial]` absent -> the `[[bus]]` entries verbatim.
    /// * `[[bus]]` absent and `[serial]` present -> one synthesized `BusConfig` named `"default"`,
    ///   carrying the legacy `[serial]`/`[poll]` sections — full backward compatibility with a
    ///   pre-#238 single-port config.
    /// * Both present -> a hard config error (ambiguous: which is authoritative?), not a silent
    ///   pick — the same "fail loudly on a config typo" posture `deny_unknown_fields` established
    ///   (see `docs/gap-analysis-meteor-parity.md`'s "Config-level partials" finding).
    /// * Neither present -> a hard config error (nothing to poll).
    ///
    /// Pure shape normalization only — it does NOT validate each bus's `poll.interval_ms` (see
    /// [`Config::validate_poll_cadence`], called separately by `runtime::supervise` after this).
    /// Keeping the two apart lets tests exercise the `[[bus]]`/`[serial]` normalization shape with
    /// arbitrary `interval_ms` fixture values without also having to satisfy the 1 Hz bound.
    pub fn buses(&self) -> anyhow::Result<Vec<BusConfig>> {
        match (self.bus.is_empty(), &self.serial) {
            (false, Some(_)) => anyhow::bail!(
                "config error: both [[bus]] and the legacy [serial] section are present; \
                 configure one or the other, not both"
            ),
            (false, None) => Ok(self.bus.clone()),
            (true, Some(serial)) => Ok(vec![BusConfig {
                name: "default".to_string(),
                serial: serial.clone(),
                poll: self.poll.clone(),
                // The legacy single-`[serial]` shape is a battery/Seplos bus under its own device
                // identity — the pre-D13 behavior, unchanged.
                protocol: ProtocolCfg::Seplos,
                device_id: None,
            }]),
            (true, None) => anyhow::bail!(
                "config error: no serial bus configured; set [serial] or at least one [[bus]]"
            ),
        }
    }

    /// Validate every bus's `poll.interval_ms` against the MIL-STD-3071 1 Hz `DcMeasurementUpdate`
    /// requirement ([`PollConfig::validate`]), failing loudly (bus name included) on the first
    /// violation. Called by `runtime::supervise` right after [`Config::buses`] resolves, i.e. on
    /// every real boot — kept as a separate step (not folded into `buses()` itself) so `buses()`
    /// stays pure shape-normalization and its unit tests can use arbitrary `interval_ms` fixture
    /// values without also satisfying this bound.
    pub fn validate_poll_cadence(buses: &[BusConfig]) -> anyhow::Result<()> {
        for bus in buses {
            bus.poll
                .validate()
                .map_err(|e| anyhow::anyhow!("bus '{}': {e}", bus.name))?;
        }
        Ok(())
    }

    /// Derive one [`IdentityCfg`] per configured bus (D13), in bus order (so it zips 1:1 with
    /// [`Config::buses`]). Each bus's `device_id` is `bus.device_id` when set, else `{device.id}`
    /// for a Seplos bus or `{device.id}-inv` for a Voltronic bus — truncated to the 32-char
    /// `Identity` bound; its `role` is derived from the protocol (Seplos → `ROLE_STORAGE`,
    /// Voltronic → `ROLE_CONVERSION`; decision 0007), NOT from `[device].role` (now
    /// accepted-but-ignored — see [`DeviceConfig::role`]).
    ///
    /// **Duplicate `device_id`s across buses are a config-load error** (two buses publishing under
    /// the same TMS `@key` would be indistinguishable) — the same fail-loudly posture as
    /// [`Config::buses`]'s "both present" / "neither present" rules.
    pub fn identities(&self) -> anyhow::Result<Vec<IdentityCfg>> {
        let buses = self.buses()?;
        let mut out: Vec<IdentityCfg> = Vec::with_capacity(buses.len());
        for bus in &buses {
            let device_id = match &bus.device_id {
                Some(id) => truncate_identity(id),
                None => match bus.protocol {
                    ProtocolCfg::Seplos => truncate_identity(&self.device.id),
                    ProtocolCfg::Voltronic => truncate_identity(&format!("{}-inv", self.device.id)),
                },
            };
            if out.iter().any(|i| i.device_id == device_id) {
                anyhow::bail!(
                    "config error: duplicate device_id '{device_id}' across buses; each bus's TMS \
                     identity must be unique (set a distinct [[bus]].device_id or [device].id)"
                );
            }
            let role = match bus.protocol {
                ProtocolCfg::Seplos => DeviceRole::Storage,
                ProtocolCfg::Voltronic => DeviceRole::Conversion,
            };
            out.push(IdentityCfg {
                device_id,
                role,
                protocol: bus.protocol,
            });
        }
        Ok(out)
    }

    /// The private-cloud bridge config, if this device pushes to the cloud at all.
    pub fn cloud(&self) -> Option<&CloudConfig> {
        self.cloud.as_ref()
    }

    pub fn broker_config(&self) -> BrokerConfig {
        BrokerConfig {
            id: "sentient".into(),
            host: self.broker.host.clone(),
            port: self.broker.port,
            // No CONNECT authorizer wired yet: `lib-security` ships no concrete
            // `TokenVerifier` backend in this workspace (see `lib_mqtt::broker::seam_auth`'s
            // doc comment for the flagged gap). A deployment that widens `broker.host` beyond
            // loopback needs a real backend bound here before that is safe.
            auth: None,
        }
    }

    /// Validate every non-bus-specific config section that has a semantic bound (see each
    /// section type's own `validate()` for the specific rule + rationale): `[broker]`, `[cloud]`
    /// (if present), `[greengrass]` (if present), `[disk]` (if present), `[capture]` (if present),
    /// `[meshtastic]` (if present). Called by `runtime::supervise` on every real boot, alongside
    /// [`Config::validate_poll_cadence`]/[`Config::validate_bus_serial`] (the per-bus checks, which
    /// need the resolved `Vec<BusConfig>` this method doesn't take). `[dds]`, if present, validates
    /// itself via `runtime.rs`'s `dds_config` (which calls `DdsToml::validate` before this method
    /// would ever be reached in `supervise`'s call order — see that function's doc comment).
    pub fn validate(&self) -> anyhow::Result<()> {
        self.broker.validate()?;
        if let Some(cloud) = &self.cloud {
            cloud.validate()?;
        }
        if let Some(greengrass) = &self.greengrass {
            greengrass.validate()?;
        }
        if let Some(disk) = &self.disk {
            disk.validate()?;
        }
        if let Some(capture) = &self.capture {
            capture.validate()?;
        }
        if let Some(meshtastic) = &self.meshtastic {
            meshtastic.validate()?;
        }
        Ok(())
    }

    /// Validate every configured bus's `serial` section ([`SerialConfig::validate`]), failing
    /// loudly (bus name included) on the first violation — the per-bus mirror of
    /// [`Config::validate_poll_cadence`] (same reason it's a separate step, not folded into
    /// [`Config::buses`]: that method stays pure shape-normalization and its own unit tests use
    /// arbitrary fixture values without also satisfying these bounds).
    pub fn validate_bus_serial(buses: &[BusConfig]) -> anyhow::Result<()> {
        for bus in buses {
            bus.serial
                .validate()
                .map_err(|e| anyhow::anyhow!("bus '{}': {e}", bus.name))?;
        }
        Ok(())
    }
}

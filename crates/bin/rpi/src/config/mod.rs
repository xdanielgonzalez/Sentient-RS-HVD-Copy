//! Runtime configuration (figment: TOML file + `SENTIENT_` env overrides).
//!
//! Everything the binary does is config-driven — device identity, which serial transport and line
//! params, the poll cadence + addresses, the embedded broker bind, and the publish prefix. Nothing
//! (endpoints, credentials, ports) is hardcoded; Day-1 Sentient values live in the TOML/env.

mod prelude;
mod types;

mod impl_broker_toml;
mod impl_capture_config;
mod impl_cloud_config;
mod impl_config;
mod impl_dds_toml;
mod impl_disk_config;
mod impl_greengrass_config;
mod impl_identity_cfg;
mod impl_location_config;
mod impl_meshtastic_config;
mod impl_poll_config;
mod impl_product_params;
mod impl_publish_config;
mod impl_serial_config;

pub use types::*;

#[cfg(test)]
use prelude::*;

#[cfg(test)]
// Test code may unwrap/panic; the workspace denies these in logic only.
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use figment::providers::{Format, Toml};
    use figment::Figment;

    fn from_toml(s: &str) -> Config {
        Figment::new()
            .merge(Toml::string(s))
            .extract()
            .expect("parse")
    }

    #[test]
    fn parses_full_config_and_maps() {
        let cfg = from_toml(
            r#"
            [device]
            id = "METEOR-001"
            [serial]
            kind = "rs485"
            port = "/dev/ttyAMA0"
            baud = 19200
            parity = "even"
            stop_bits = 2
            [poll]
            dip_addresses = [0, 1, 2]
            interval_ms = 1500
            fault_every = 5
            [broker]
            host = "127.0.0.1"
            port = 1884
            [publish]
            prefix = "tms"
            "#,
        );
        assert_eq!(cfg.device.id, "METEOR-001");
        let serial = cfg.serial.as_ref().expect("legacy [serial] present");
        assert!(matches!(serial.serial_kind(), SerialKind::Rs485));
        let p = serial.serial_params();
        assert_eq!(p.baud, 19200);
        assert!(matches!(p.parity, Parity::Even));
        assert!(matches!(p.stop_bits, StopBits::Two));
        assert_eq!(cfg.poll.dip_addresses, vec![0, 1, 2]);
        assert_eq!(cfg.poll.fault_every, 5);
        assert_eq!(cfg.broker_config().port, 1884);
    }

    #[test]
    fn applies_defaults_for_optional_sections() {
        let cfg = from_toml(
            r#"
            [device]
            id = "dev"
            [serial]
            kind = "rs232"
            port = "/dev/ttyUSB0"
            "#,
        );
        // serial defaults
        let serial = cfg.serial.as_ref().expect("legacy [serial] present");
        assert_eq!(serial.serial_params().baud, 19200);
        assert!(matches!(serial.serial_params().stop_bits, StopBits::One));
        // section defaults — 1 s is the TMS-required nominal DcMeasurementUpdate cadence (§4.9.10.4).
        assert_eq!(cfg.poll.interval_ms, 1000);
        assert_eq!(cfg.broker.host, "127.0.0.1");
        assert_eq!(cfg.broker.port, 1883);
        assert_eq!(cfg.publish.prefix, "tms");
    }

    #[test]
    // Mutates the process-global `SENTIENT_*` env — must not race other tests that merge the same
    // `Env::prefixed("SENTIENT_")` provider (see `cloud_section_env_override`).
    #[serial_test::serial(sentient_env)]
    fn env_overrides_toml() {
        use figment::providers::Env;
        temp_env_var("SENTIENT_SERIAL__PORT", "/dev/ttyS9", || {
            let cfg: Config = Figment::new()
                .merge(Toml::string(
                    "[device]\nid=\"d\"\n[serial]\nkind=\"rs232\"\nport=\"/dev/ttyAMA0\"\n",
                ))
                .merge(Env::prefixed("SENTIENT_").split("__"))
                .extract()
                .expect("parse");
            assert_eq!(
                cfg.serial.as_ref().expect("serial present").port,
                "/dev/ttyS9"
            );
        });
    }

    #[test]
    fn explicit_wide_bind_overrides_loopback_default() {
        // A deployment may deliberately widen the bind address; only the DEFAULT is loopback.
        let cfg = from_toml(
            r#"
            [device]
            id = "dev"
            [serial]
            kind = "rs232"
            port = "/dev/ttyUSB0"
            [broker]
            host = "0.0.0.0"
            port = 1883
            "#,
        );
        assert_eq!(cfg.broker.host, "0.0.0.0");
    }

    fn temp_env_var(key: &str, val: &str, f: impl FnOnce()) {
        std::env::set_var(key, val);
        f();
        std::env::remove_var(key);
    }

    #[test]
    fn cloud_section_absent_by_default() {
        let cfg = from_toml(
            r#"
            [device]
            id = "dev"
            [serial]
            kind = "rs232"
            port = "/dev/ttyUSB0"
            "#,
        );
        assert!(cfg.cloud().is_none());
    }

    #[test]
    fn cloud_section_parses_from_toml() {
        let cfg = from_toml(
            r#"
            [device]
            id = "dev"
            [serial]
            kind = "rs232"
            port = "/dev/ttyUSB0"
            [cloud]
            host = "mqtt.sentient.example"
            port = 8883
            username = "meteor-001"
            ca_file = "/etc/sentient/cloud-ca.pem"
            remote_clientid = "meteor-001-bridge"
            "#,
        );
        let cloud = cfg.cloud().expect("cloud section present");
        assert_eq!(cloud.host, "mqtt.sentient.example");
        assert_eq!(cloud.port, 8883);
        assert_eq!(cloud.username, "meteor-001");
        assert_eq!(cloud.ca_file, "/etc/sentient/cloud-ca.pem");
        assert_eq!(cloud.remote_clientid.as_deref(), Some("meteor-001-bridge"));
        // defaults
        assert_eq!(cloud.topic, "tms/#");
        assert_eq!(cloud.qos, 1);
        // no password field exists on the struct at all — nothing to assert absent-of, by
        // construction; see the CLOUD_MQTT_PASS fixture tests in mosquitto_bridge.rs.
    }

    #[test]
    #[serial_test::serial(sentient_env)]
    fn cloud_section_env_override() {
        temp_env_var("SENTIENT_CLOUD__HOST", "override.example", || {
            let cfg: Config = Figment::new()
                .merge(Toml::string(
                    "[device]\nid=\"d\"\n[serial]\nkind=\"rs232\"\nport=\"/dev/ttyUSB0\"\n\
                     [cloud]\nhost=\"orig.example\"\nusername=\"u\"\nca_file=\"/ca.pem\"\n",
                ))
                .merge(figment::providers::Env::prefixed("SENTIENT_").split("__"))
                .extract()
                .expect("parse");
            assert_eq!(cfg.cloud().expect("cloud").host, "override.example");
        });
    }

    #[test]
    fn cloud_env_map_excludes_password() {
        let cfg = from_toml(
            r#"
            [device]
            id = "dev"
            [serial]
            kind = "rs232"
            port = "/dev/ttyUSB0"
            [cloud]
            host = "mqtt.sentient.example"
            username = "meteor-001"
            ca_file = "/etc/sentient/cloud-ca.pem"
            "#,
        );
        let map = cfg.cloud().expect("cloud").env_map();
        assert!(map
            .iter()
            .any(|(k, v)| *k == "CLOUD_MQTT_HOST" && v == "mqtt.sentient.example"));
        assert!(map
            .iter()
            .any(|(k, v)| *k == "CLOUD_MQTT_USER" && v == "meteor-001"));
        assert!(map
            .iter()
            .any(|(k, v)| *k == "CLOUD_MQTT_CAFILE" && v == "/etc/sentient/cloud-ca.pem"));
        assert!(!map.iter().any(|(k, _)| *k == "CLOUD_MQTT_PASS"));
    }

    #[test]
    // Mutates the process-global `CLOUD_MQTT_PASS` env — serialize against
    // `mosquitto_bridge::tests::read_password_secret_reads_the_documented_env_var`, the other test
    // that touches this same var.
    #[serial_test::serial(cloud_mqtt_pass_env)]
    fn debug_output_never_contains_the_password() {
        let cfg = from_toml(
            r#"
            [device]
            id = "dev"
            [serial]
            kind = "rs232"
            port = "/dev/ttyUSB0"
            [cloud]
            host = "mqtt.sentient.example"
            username = "meteor-001"
            ca_file = "/etc/sentient/cloud-ca.pem"
            "#,
        );
        // The secret never enters the struct, so it cannot leak through Debug/logging no matter
        // what the real deployment's CLOUD_MQTT_PASS value is. Prove it with a canary: set a
        // distinctive "secret" in the environment and assert it never shows up in `{:?}`.
        temp_env_var(
            "CLOUD_MQTT_PASS",
            "sekret-canary-value-should-not-leak",
            || {
                let dump = format!("{cfg:?}");
                assert!(!dump.contains("sekret-canary-value-should-not-leak"));
            },
        );
    }

    #[test]
    fn unknown_top_level_key_is_rejected() {
        // `deny_unknown_fields` (gap-closure #237, "Config-level partials" finding): a typo'd or
        // stale config key must fail loudly at startup, never be silently ignored.
        let result: Result<Config, _> = Figment::new()
            .merge(Toml::string(
                r#"
                [device]
                id = "dev"
                [serial]
                kind = "rs232"
                port = "/dev/ttyUSB0"
                this_key_does_not_exist = "nope"
                "#,
            ))
            .extract();
        assert!(
            result.is_err(),
            "an unrecognized config key must be rejected, not silently ignored"
        );
    }

    #[test]
    fn legacy_single_serial_config_still_accepted() {
        // The pre-#237 config shape (no `[bus]`/`[disk]`/`[greengrass]`/`[meshtastic]`/
        // `[location]`/`direction`) must keep loading unchanged under the new
        // `deny_unknown_fields` + new-optional-sections Config.
        let cfg = from_toml(
            r#"
            [device]
            id = "METEOR-001"
            [serial]
            kind = "rs485"
            port = "/dev/ttyAMA0"
            baud = 19200
            "#,
        );
        assert_eq!(cfg.device.id, "METEOR-001");
        assert!(cfg.bus.is_empty());
        assert!(cfg.disk.is_none());
        assert!(cfg.greengrass.is_none());
        assert!(cfg.meshtastic.is_none());
        assert!(cfg.location.is_none());
        assert!(cfg.direction.is_none());
        assert!(cfg.capture.is_none());
    }

    // ---- CaptureConfig / [capture] section (raw wire-capture-to-disk stub, decision 0001) -----

    #[test]
    fn capture_section_absent_by_default() {
        let cfg = from_toml(
            r#"
            [device]
            id = "dev"
            [serial]
            kind = "rs232"
            port = "/dev/ttyUSB0"
            "#,
        );
        assert!(cfg.capture.is_none());
    }

    #[test]
    fn capture_section_parses_required_dir_and_applies_defaults() {
        let cfg = from_toml(
            r#"
            [device]
            id = "dev"
            [serial]
            kind = "rs232"
            port = "/dev/ttyUSB0"
            [capture]
            dir = "/var/lib/sentient/capture"
            "#,
        );
        let capture = cfg.capture.as_ref().expect("[capture] section present");
        assert_eq!(capture.dir, "/var/lib/sentient/capture");
        assert_eq!(capture.max_file_bytes, 10 * 1024 * 1024);
        assert_eq!(capture.max_total_bytes, 100 * 1024 * 1024);
        assert_eq!(capture.queue_depth, 256);
    }

    #[test]
    fn capture_section_overrides_thread_through() {
        let cfg = from_toml(
            r#"
            [device]
            id = "dev"
            [serial]
            kind = "rs232"
            port = "/dev/ttyUSB0"
            [capture]
            dir = "/mnt/usb/capture"
            max_file_bytes = 1048576
            max_total_bytes = 5242880
            queue_depth = 16
            "#,
        );
        let capture = cfg.capture.as_ref().expect("[capture] section present");
        assert_eq!(capture.dir, "/mnt/usb/capture");
        assert_eq!(capture.max_file_bytes, 1_048_576);
        assert_eq!(capture.max_total_bytes, 5_242_880);
        assert_eq!(capture.queue_depth, 16);
    }

    #[test]
    fn capture_section_without_dir_fails_to_parse() {
        // `dir` has no default (unlike every other `CaptureConfig` field) — presence of
        // `[capture]` is the opt-in signal, but `dir` must still be explicit.
        let result: Result<Config, _> = Figment::new()
            .merge(Toml::string(
                r#"
                [device]
                id = "dev"
                [serial]
                kind = "rs232"
                port = "/dev/ttyUSB0"
                [capture]
                max_file_bytes = 1024
                "#,
            ))
            .extract();
        assert!(
            result.is_err(),
            "[capture] without dir must fail to parse, not silently default"
        );
    }

    #[test]
    fn capture_section_rejects_unknown_fields() {
        let result: Result<Config, _> = Figment::new()
            .merge(Toml::string(
                r#"
                [device]
                id = "dev"
                [serial]
                kind = "rs232"
                port = "/dev/ttyUSB0"
                [capture]
                dir = "/var/lib/sentient/capture"
                bogus_key = "nope"
                "#,
            ))
            .extract();
        assert!(
            result.is_err(),
            "an unrecognized [capture] key must be rejected, not silently ignored"
        );
    }

    // ---- Config::buses() normalization (#238/#242 multi-port) --------------------------------

    #[test]
    fn buses_normalizes_legacy_single_serial_to_one_default_bus() {
        let cfg = from_toml(
            r#"
            [device]
            id = "METEOR-001"
            [serial]
            kind = "rs485"
            port = "/dev/ttyAMA0"
            baud = 19200
            [poll]
            dip_addresses = [0, 1]
            interval_ms = 1500
            fault_every = 5
            "#,
        );
        let buses = cfg.buses().expect("legacy single-serial config normalizes");
        assert_eq!(buses.len(), 1);
        assert_eq!(buses[0].name, "default");
        assert_eq!(buses[0].serial.port, "/dev/ttyAMA0");
        assert_eq!(buses[0].poll.dip_addresses, vec![0, 1]);
        assert_eq!(buses[0].poll.interval_ms, 1500);
    }

    #[test]
    fn buses_returns_configured_bus_array_entries_verbatim() {
        let cfg = from_toml(
            r#"
            [device]
            id = "METEOR-001"
            [[bus]]
            name = "pack-a"
            [bus.serial]
            kind = "rs485"
            port = "/dev/ttyUSB0"
            [bus.poll]
            dip_addresses = [0]
            interval_ms = 2000
            fault_every = 10
            [[bus]]
            name = "pack-b"
            [bus.serial]
            kind = "rs485"
            port = "/dev/ttyUSB1"
            [bus.poll]
            dip_addresses = [1]
            interval_ms = 2000
            fault_every = 10
            "#,
        );
        let buses = cfg.buses().expect("[[bus]] array normalizes");
        assert_eq!(buses.len(), 2);
        assert_eq!(buses[0].name, "pack-a");
        assert_eq!(buses[0].serial.port, "/dev/ttyUSB0");
        assert_eq!(buses[1].name, "pack-b");
        assert_eq!(buses[1].serial.port, "/dev/ttyUSB1");
    }

    #[test]
    fn buses_errors_when_both_bus_array_and_legacy_serial_present() {
        let cfg = from_toml(
            r#"
            [device]
            id = "METEOR-001"
            [serial]
            kind = "rs485"
            port = "/dev/ttyAMA0"
            [[bus]]
            name = "pack-a"
            [bus.serial]
            kind = "rs485"
            port = "/dev/ttyUSB0"
            "#,
        );
        let result = cfg.buses();
        assert!(
            result.is_err(),
            "both [[bus]] and legacy [serial] present must be a config error, never a silent pick"
        );
    }

    #[test]
    fn buses_errors_when_neither_bus_array_nor_legacy_serial_present() {
        let cfg = from_toml(
            r#"
            [device]
            id = "METEOR-001"
            "#,
        );
        let result = cfg.buses();
        assert!(
            result.is_err(),
            "no serial bus at all must be a config error, not an empty poll loop"
        );
    }

    // ---- PollConfig::validate() / Config::validate_poll_cadence() (MIL-STD-3071 1 Hz guard) ---
    //
    // `DcMeasurementUpdate` is NOT rate-gated on the emit path (`bin_rpi_platform::EmitCadence`),
    // so `[poll].interval_ms` directly sets its publish rate. These prove the regression this
    // task fixes (every shipped config at `interval_ms = 2000` = 0.5 Hz) cannot recur silently.

    #[test]
    fn poll_config_validate_accepts_the_1hz_boundary() {
        let cfg = PollConfig {
            dip_addresses: vec![0],
            interval_ms: MAX_INTERVAL_MS_FOR_1HZ,
            fault_every: 10,
        };
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn poll_config_validate_rejects_slower_than_1hz() {
        let cfg = PollConfig {
            dip_addresses: vec![0],
            // The exact value every shipped config regressed to before this fix.
            interval_ms: 2000,
            fault_every: 10,
        };
        let err = cfg
            .validate()
            .expect_err("2000ms interval must fail the 1 Hz check");
        assert!(
            err.to_string().contains("1 Hz"),
            "error should explain the 1 Hz requirement: {err}"
        );
    }

    #[test]
    fn poll_config_validate_rejects_zero() {
        let cfg = PollConfig {
            dip_addresses: vec![0],
            interval_ms: 0,
            fault_every: 10,
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn poll_config_validate_rejects_empty_dip_addresses() {
        // Empty parses fine as a `Vec<u8>` but polls nothing every cycle — no operator intends
        // that; see `PollConfig::validate`.
        let cfg = PollConfig {
            dip_addresses: vec![],
            interval_ms: MAX_INTERVAL_MS_FOR_1HZ,
            fault_every: 10,
        };
        let err = cfg
            .validate()
            .expect_err("empty dip_addresses must be rejected");
        assert!(err.to_string().contains("dip_addresses"), "{err}");
    }

    #[test]
    fn poll_config_validate_accepts_the_max_seplos_dip_address() {
        let cfg = PollConfig {
            dip_addresses: vec![MAX_SEPLOS_DIP_ADDRESS],
            interval_ms: MAX_INTERVAL_MS_FOR_1HZ,
            fault_every: 10,
        };
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn poll_config_validate_rejects_a_dip_address_above_the_seplos_encodable_range() {
        // The Seplos protocol packs the DIP address into one hex digit (0..=15) —
        // `SeplosError::AddrOutOfRange` above that; see `PollConfig::validate`.
        let cfg = PollConfig {
            dip_addresses: vec![0, MAX_SEPLOS_DIP_ADDRESS + 1],
            interval_ms: MAX_INTERVAL_MS_FOR_1HZ,
            fault_every: 10,
        };
        let err = cfg.validate().expect_err("dip address 16 must be rejected");
        assert!(err.to_string().contains("16"), "{err}");
    }

    #[test]
    fn validate_poll_cadence_fails_loudly_for_an_out_of_spec_bus() {
        let cfg = from_toml(
            r#"
            [device]
            id = "METEOR-001"
            [serial]
            kind = "rs485"
            port = "/dev/ttyAMA0"
            [poll]
            dip_addresses = [0]
            interval_ms = 2000
            fault_every = 10
            "#,
        );
        let buses = cfg.buses().expect("legacy single-serial config normalizes");
        let err = Config::validate_poll_cadence(&buses)
            .expect_err("interval_ms=2000 must fail Config::validate_poll_cadence");
        assert!(
            err.to_string().contains("default"),
            "error should name the offending bus: {err}"
        );
    }

    #[test]
    fn validate_poll_cadence_passes_for_the_1000ms_default() {
        let cfg = from_toml(
            r#"
            [device]
            id = "METEOR-001"
            [serial]
            kind = "rs485"
            port = "/dev/ttyAMA0"
            [poll]
            dip_addresses = [0]
            interval_ms = 1000
            fault_every = 10
            "#,
        );
        let buses = cfg.buses().expect("legacy single-serial config normalizes");
        assert!(Config::validate_poll_cadence(&buses).is_ok());
    }

    // ---- Config::identities() per-bus identity derivation (D13, decision 0007) ---------------

    #[test]
    fn identities_derives_storage_and_conversion_per_protocol() {
        use lib_mapping::schema::DeviceRole;
        let cfg = from_toml(
            r#"
            [device]
            id = "METEOR-001"
            [[bus]]
            name = "battery"
            [bus.serial]
            kind = "rs485"
            port = "/dev/ttySC0"
            [[bus]]
            name = "inverter"
            protocol = "voltronic"
            [bus.serial]
            kind = "rs232"
            port = "/dev/ttySC1"
            "#,
        );
        let ids = cfg.identities().expect("identities derive");
        assert_eq!(ids.len(), 2);
        // Seplos bus -> ROLE_STORAGE under the plain device id.
        assert_eq!(ids[0].device_id, "METEOR-001");
        assert_eq!(ids[0].role, DeviceRole::Storage);
        assert_eq!(ids[0].device_type(), "battery");
        // Voltronic bus -> ROLE_CONVERSION under the `-inv`-suffixed id.
        assert_eq!(ids[1].device_id, "METEOR-001-inv");
        assert_eq!(ids[1].role, DeviceRole::Conversion);
        assert_eq!(ids[1].device_type(), "inverter");
    }

    #[test]
    fn identities_uses_explicit_bus_device_id_override() {
        let cfg = from_toml(
            r#"
            [device]
            id = "METEOR-001"
            [[bus]]
            name = "inverter"
            protocol = "voltronic"
            device_id = "INV-CUSTOM"
            [bus.serial]
            kind = "rs232"
            port = "/dev/ttySC1"
            "#,
        );
        let ids = cfg.identities().expect("identities derive");
        assert_eq!(ids.len(), 1);
        assert_eq!(
            ids[0].device_id, "INV-CUSTOM",
            "an explicit [[bus]].device_id overrides the derived default"
        );
    }

    #[test]
    fn identities_errors_on_duplicate_device_id_across_buses() {
        // Two Seplos buses with no explicit device_id both derive `{device.id}` — a duplicate.
        let cfg = from_toml(
            r#"
            [device]
            id = "METEOR-001"
            [[bus]]
            name = "pack-a"
            [bus.serial]
            kind = "rs485"
            port = "/dev/ttyUSB0"
            [[bus]]
            name = "pack-b"
            [bus.serial]
            kind = "rs485"
            port = "/dev/ttyUSB1"
            "#,
        );
        assert!(
            cfg.identities().is_err(),
            "duplicate derived device_id across buses must be a config-load error"
        );
    }

    #[test]
    fn identities_legacy_single_serial_is_one_storage_identity() {
        use lib_mapping::schema::DeviceRole;
        let cfg = from_toml(
            r#"
            [device]
            id = "METEOR-001"
            [serial]
            kind = "rs485"
            port = "/dev/ttyAMA0"
            "#,
        );
        let ids = cfg
            .identities()
            .expect("legacy serial derives one identity");
        assert_eq!(ids.len(), 1);
        assert_eq!(ids[0].device_id, "METEOR-001");
        assert_eq!(ids[0].role, DeviceRole::Storage);
    }

    #[test]
    // Mutates the process-global `SENTIENT_*` env — must not race `env_overrides_toml`/
    // `cloud_section_env_override`, the other tests merging the same `Env::prefixed("SENTIENT_")`
    // provider.
    #[serial_test::serial(sentient_env)]
    fn sentient_config_env_var_coexists_with_deny_unknown_fields() {
        // `SENTIENT_CONFIG` (the config-file-path override `main.rs` reads directly via
        // `std::env::var`, never a TOML key) must not be rejected by `Config`'s
        // `deny_unknown_fields` once figment's `Env::prefixed` provider picks it up as a
        // top-level `config` key — see `Config::load`'s `.ignore(&["config"])`.
        let dir = std::env::temp_dir().join(format!("bin-rpi-cfgtest-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let toml_path = dir.join("sentient.toml");
        std::fs::write(
            &toml_path,
            r#"
            [device]
            id = "dev"
            [serial]
            kind = "rs232"
            port = "/dev/ttyUSB0"
            "#,
        )
        .expect("write fixture toml");
        let path_str = toml_path.to_str().expect("utf8 path").to_string();

        temp_env_var("SENTIENT_CONFIG", &path_str, || {
            let result = Config::load(Some(&path_str));
            assert!(
                result.is_ok(),
                "SENTIENT_CONFIG must coexist with deny_unknown_fields: {:?}",
                result.err()
            );
        });

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- distro/deploy/config.example.toml cross-check (#237 integration pass) ---------------

    /// Uncomment every real commented-out TOML key/section-header line in `src` (the deployment
    /// example, which ships with every optional/secondary section commented out — see that
    /// file's own doc comment), leaving pure prose comment lines alone. Distinguishes the two by
    /// a simple, deliberately conservative rule that matches this file's actual formatting
    /// convention: a candidate line starts with EXACTLY `"# "` (one space, no further indent —
    /// prose continuation lines in this file are indented further, e.g. `"#   SENTIENT_..."`/
    /// `"#                          # proxying..."`), and what follows is either a bare section
    /// header (`[section]`/`[[array]]`, nothing else on the line) or a `key = value` pair (an
    /// identifier immediately before the first `=`). Verified by [`example_config_every_optional_section_parses`]
    /// below actually round-tripping the transformed output through the real [`Config`] type.
    fn uncomment_example_config(src: &str) -> String {
        fn is_section_header(t: &str) -> bool {
            let t = t.trim_end();
            let Some(inner) = t.strip_prefix('[').and_then(|s| s.strip_suffix(']')) else {
                return false;
            };
            let inner = inner.trim_start_matches('[').trim_end_matches(']');
            !inner.is_empty()
                && inner
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
        }
        fn is_key_value(rest: &str) -> bool {
            let Some((key, _)) = rest.split_once('=') else {
                return false;
            };
            let key = key.trim();
            !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        }

        src.lines()
            .map(|line| match line.strip_prefix("# ") {
                // Extra indent after "# " (rest != its own left-trim) marks a prose continuation
                // line (e.g. a wrapped sentence), never a real commented-out TOML line in this
                // file's convention — leave it alone.
                Some(rest)
                    if rest == rest.trim_start()
                        && (is_section_header(rest) || is_key_value(rest)) =>
                {
                    rest
                }
                _ => line,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The deployment example (`distro/deploy/config.example.toml`) ships with every
    /// optional/secondary section (`[disk]`, `[meshtastic]`, `[greengrass]`, `[location]`, the
    /// top-level `direction` selector, the `[[bus]]` alternative, every optional `[device]`/
    /// `[serial]`/`[cloud]` field) commented out at its documented default. This test uncomments
    /// all of them (see [`uncomment_example_config`]) and feeds the result through the SAME
    /// `Figment` + `Config` (`deny_unknown_fields`) extraction path `Config::load` uses, proving
    /// every field name in the shipped example actually matches the real config surface — a
    /// stale/renamed field here would fail loudly (`deny_unknown_fields`) rather than silently
    /// drift, which is exactly what caught the `[meshtastic]` section's stale
    /// `device_path`/`max_payload`/`send_interval_ms` field names during this integration pass
    /// (renamed to `port`/`max_bytes`/`min_send_interval_ms` — see `MeshtasticConfig`).
    ///
    /// `[[bus]]` and the legacy `[serial]`/`[poll]` pair are BOTH present once fully uncommented
    /// (they're documented as mutually exclusive alternatives within the file) — `Config`
    /// extraction itself still succeeds (both fields simply populate), so this test only checks
    /// extraction + a field spot-check, not `Config::buses()` (which correctly rejects "both
    /// present" — see `buses_errors_when_both_bus_array_and_legacy_serial_present` above).
    #[test]
    fn example_config_every_optional_section_parses() {
        let raw = include_str!("../../../../../distro/deploy/config.example.toml");
        let uncommented = uncomment_example_config(raw);

        let cfg: Config = Figment::new()
            .merge(Toml::string(&uncommented))
            .extract()
            .unwrap_or_else(|e| {
                panic!(
                    "distro/deploy/config.example.toml, fully uncommented, must parse against \
                     the real Config type (deny_unknown_fields catches any stale/renamed field): \
                     {e}\n---\n{uncommented}"
                )
            });

        assert_eq!(cfg.device.role, "STOR");
        assert_eq!(cfg.device.manufacturer, "Sentient");
        assert!(matches!(cfg.direction, Some(DirectionCfg::Rts)));
        let disk = cfg.disk.as_ref().expect("[disk] section present");
        assert_eq!(disk.dir, "/var/lib/sentient/telemetry");
        assert_eq!(disk.max_files, 10);
        let mesh = cfg
            .meshtastic
            .as_ref()
            .expect("[meshtastic] section present");
        assert!(mesh.enabled);
        assert_eq!(mesh.port, "/dev/ttyUSB1");
        assert_eq!(mesh.max_bytes, 200);
        assert_eq!(mesh.min_send_interval_ms, 30_000);
        let gg = cfg
            .greengrass
            .as_ref()
            .expect("[greengrass] section present");
        assert_eq!(gg.host, "gg-core.local");
        assert_eq!(gg.component_name, "");
        let loc = cfg.location.as_ref().expect("[location] section present");
        assert_eq!(loc.order.len(), 4);
        assert_eq!(loc.manual_lat, Some(37.774929));
        assert!(!loc.ip_geo_enabled);
        assert_eq!(cfg.bus.len(), 2, "both [[bus]] entries uncommented");
        assert_eq!(cfg.bus[0].name, "pack-a");
        assert_eq!(cfg.bus[1].name, "pack-b");
        assert!(
            cfg.serial.is_some(),
            "legacy [serial] stays present too (mutually-exclusive-in-practice, both parse)"
        );
        // Regression guard: the shipped example's poll cadence must actually MEET the
        // MIL-STD-3071 1 Hz `DcMeasurementUpdate` requirement, not merely parse as a u64 — a
        // config with `interval_ms = 2000` parses fine but silently ships at half the spec rate
        // (see `PollConfig::validate`). Every poll section in this file (legacy [poll] + both
        // [[bus]] entries) must be at-or-under the 1 Hz bound.
        assert_eq!(
            cfg.poll.interval_ms, 1000,
            "legacy [poll].interval_ms in the shipped example must meet the 1 Hz \
             DcMeasurementUpdate requirement"
        );
        assert!(cfg.poll.validate().is_ok());
        for bus in &cfg.bus {
            assert_eq!(
                bus.poll.interval_ms, 1000,
                "[[bus]] '{}' interval_ms in the shipped example must meet the 1 Hz \
                 DcMeasurementUpdate requirement",
                bus.name
            );
            assert!(bus.poll.validate().is_ok());
        }

        // `Config::buses()` itself correctly refuses this combination — this test only proves
        // every field NAME/shape in the example is valid, not that this exact merged shape is a
        // deployable config (it deliberately isn't, on purpose, in this uncomment-everything form).
        assert!(cfg.buses().is_err());
    }

    // ---- Every shipped config that becomes a REAL `sentient.toml` (MIL-STD-3071 spec gate) ----
    //
    // The defect this closes: `deploy/charts/sentient-viz/values.yaml`, `deploy/compose/configs/
    // sentient.toml`, and `deploy/dds-conformance/configs/sentient-dds.toml` all shipped
    // `interval_ms = 2000` (0.5 Hz) — half the MIL-STD-3071 `DcMeasurementUpdate` requirement —
    // and NOTHING caught it, because `example_config_every_optional_section_parses` above only
    // ever loaded `distro/deploy/config.example.toml` (a template, not a config any real
    // deployment actually runs) and, until recently, didn't even assert `interval_ms` on that one
    // file. A config can parse perfectly (a valid u64) while still violating the spec.
    //
    // This test closes that gap generally: every checked-in TOML file that is loaded VERBATIM as
    // a real `sentient.toml` by a real deployment path (Yocto image, docker-compose demo stack,
    // DDS conformance harness) is `include_str!`-ed into ONE list and run through the SAME
    // spec-constraint check. Adding a new shipped config without adding it here is intentionally
    // impossible to do silently: `include_str!` fails the BUILD (not just the test) if a listed
    // path goes missing, and any config left OFF this list gets zero spec coverage by
    // construction — the list itself, not per-file copy-pasted tests, is what has to grow.
    //
    // NOT covered here: `deploy/charts/sentient-viz/values.yaml` — it's YAML (Helm values), not a
    // TOML `sentient.toml`, so it can't be parsed by `Config`/figment. Its `poll.intervalMs` is
    // instead asserted directly against the raw file text by
    // `viz_chart_values_poll_interval_meets_1hz_spec` below, and (at deploy time) by rendering the
    // chart (`helm template`) and eyeballing the resulting `interval_ms` in the configmap.

    /// One entry per checked-in TOML file that ships as a real, directly-loadable `sentient.toml`
    /// (i.e. NOT `distro/deploy/config.example.toml`, which is a documentation template that ships
    /// every optional section commented out — see [`uncomment_example_config`] for how that one is
    /// exercised instead). Each `include_str!` path is fixed at compile time, so removing or
    /// renaming a listed file breaks the BUILD, not just this test.
    fn shipped_real_configs() -> Vec<(&'static str, &'static str)> {
        vec![
            (
                "config/sentient.toml",
                include_str!("../../../../../config/sentient.toml"),
            ),
            (
                "distro/meta-sentient/recipes-sentient/sentient-agent/files/sentient.toml",
                include_str!(
                    "../../../../../distro/meta-sentient/recipes-sentient/sentient-agent/files/sentient.toml"
                ),
            ),
            (
                "deploy/compose/configs/sentient.toml",
                include_str!("../../../../../deploy/compose/configs/sentient.toml"),
            ),
            (
                "deploy/dds-conformance/configs/sentient-dds.toml",
                include_str!("../../../../../deploy/dds-conformance/configs/sentient-dds.toml"),
            ),
        ]
    }

    /// Assert every MIL-STD-3071 spec constraint this config layer currently enforces holds for
    /// `cfg`: the legacy `[poll]` section, every `[[bus]] poll` section, and — where present —
    /// `[dds].domain_id`. Shared by every shipped-config gate below so the check itself can't
    /// silently diverge between call sites.
    fn assert_spec_constraints(name: &str, cfg: &Config) {
        cfg.poll.validate().unwrap_or_else(|e| {
            panic!("{name}: legacy [poll] fails the MIL-STD-3071 1 Hz gate: {e}")
        });
        for bus in &cfg.bus {
            bus.poll.validate().unwrap_or_else(|e| {
                panic!(
                    "{name}: [[bus]] '{}' fails the MIL-STD-3071 1 Hz gate: {e}",
                    bus.name
                )
            });
        }
        if let Some(dds) = &cfg.dds {
            dds.validate().unwrap_or_else(|e| {
                panic!("{name}: [dds] fails the MIL-STD-3071 domain-0 gate: {e}")
            });
        }
    }

    #[test]
    fn every_shipped_real_config_meets_mil_std_3071_spec_constraints() {
        let configs = shipped_real_configs();
        assert!(
            !configs.is_empty(),
            "the shipped-config list must not be empty — an empty list would silently cover \
             nothing while still reporting green"
        );
        for (name, raw) in configs {
            assert!(
                !raw.trim().is_empty(),
                "{name}: shipped config file is empty"
            );
            let cfg: Config = Figment::new()
                .merge(Toml::string(raw))
                .extract()
                .unwrap_or_else(|e| panic!("{name}: must parse as a real Config: {e}"));
            assert_spec_constraints(name, &cfg);
        }
    }

    /// `distro/deploy/config.example.toml` is a documentation template (every optional section
    /// shipped commented out), not a config any deployment loads verbatim — [`shipped_real_configs`]
    /// deliberately excludes it. It still gets the same spec gate, via the same
    /// uncomment-then-parse path [`example_config_every_optional_section_parses`] uses above.
    #[test]
    fn example_config_template_meets_mil_std_3071_spec_constraints() {
        let raw = include_str!("../../../../../distro/deploy/config.example.toml");
        let uncommented = uncomment_example_config(raw);
        let cfg: Config = Figment::new()
            .merge(Toml::string(&uncommented))
            .extract()
            .unwrap_or_else(|e| {
                panic!("distro/deploy/config.example.toml, fully uncommented, must parse: {e}")
            });
        assert_spec_constraints("distro/deploy/config.example.toml", &cfg);
    }

    /// `deploy/charts/sentient-viz/values.yaml` is YAML (Helm values), not a `sentient.toml` —
    /// `Config`/figment can't parse it, so it can't join [`shipped_real_configs`]'s loop. This
    /// asserts the raw `intervalMs:` line directly against the same 1 Hz bound
    /// `PollConfig::validate` enforces, so a regression here (e.g. someone reverting to `2000`)
    /// still fails loudly instead of silently escaping every TOML-driven check in this file.
    /// End-to-end confirmation that the value actually reaches the rendered `sentient.toml` is
    /// `helm template deploy/charts/sentient-viz` (see the task's verify step) — a static
    /// template-render isn't easily unit-tested here without pulling in a YAML+Helm-template
    /// engine, which this crate does not otherwise depend on.
    #[test]
    fn viz_chart_values_poll_interval_meets_1hz_spec() {
        let raw = include_str!("../../../../../deploy/charts/sentient-viz/values.yaml");
        let line = raw
            .lines()
            .find(|l| l.trim_start().starts_with("intervalMs:"))
            .unwrap_or_else(|| {
                panic!(
                    "deploy/charts/sentient-viz/values.yaml must have an `intervalMs:` key under \
                     app.poll — this test's line-match is stale if the key was renamed"
                )
            });
        let value_str = line
            .split(':')
            .nth(1)
            .expect("intervalMs: line must have a value after the colon")
            .split('#')
            .next()
            .expect("split always yields at least one segment")
            .trim();
        let interval_ms: u64 = value_str
            .parse()
            .unwrap_or_else(|e| panic!("intervalMs value `{value_str}` must parse as u64: {e}"));
        assert!(
            interval_ms > 0 && interval_ms <= MAX_INTERVAL_MS_FOR_1HZ,
            "deploy/charts/sentient-viz/values.yaml: app.poll.intervalMs = {interval_ms} \
             violates the MIL-STD-3071 1 Hz DcMeasurementUpdate requirement — set it to \
             1..={MAX_INTERVAL_MS_FOR_1HZ}"
        );
    }

    // ---- SerialConfig::direction_mode() precedence + mapping (#245 wiring) --------------------
    //
    // The mechanism itself (RtsSoftware/KernelRs485/GpioDe actually driving a transceiver) is
    // proven end-to-end by `lib-io/tests/direction_mode.rs` — these tests only cover the
    // config-side precedence/mapping `direction_mode()` adds, not the mechanism.

    #[test]
    fn direction_mode_per_bus_override_wins_over_top_level() {
        // Top-level says "kernel"; this port explicitly overrides to "gpio" — the per-port value
        // must win.
        let cfg = from_toml(
            r#"
            direction = "kernel"
            [device]
            id = "dev"
            [serial]
            kind = "rs485"
            port = "/dev/ttyAMA0"
            direction = "gpio"
            gpio_chip = "gpiochip4"
            gpio_line = 22
            "#,
        );
        let serial = cfg.serial.as_ref().expect("serial present");
        let mode = serial
            .direction_mode(cfg.direction)
            .expect("gpio_chip/gpio_line both present");
        match mode {
            DirectionMode::GpioDe { chip, line, .. } => {
                assert_eq!(chip, "gpiochip4");
                assert_eq!(line, 22);
            }
            other => panic!("expected GpioDe (per-bus override must win), got {other:?}"),
        }
    }

    #[test]
    fn direction_mode_falls_back_to_top_level_when_per_bus_unset() {
        // This port sets no `direction` of its own — the top-level selector applies.
        let cfg = from_toml(
            r#"
            direction = "kernel"
            [device]
            id = "dev"
            [serial]
            kind = "rs485"
            port = "/dev/ttyAMA0"
            "#,
        );
        let serial = cfg.serial.as_ref().expect("serial present");
        let mode = serial
            .direction_mode(cfg.direction)
            .expect("kernel mode never fails to map");
        assert!(
            matches!(mode, DirectionMode::KernelRs485),
            "expected the top-level fallback (KernelRs485), got {mode:?}"
        );
    }

    #[test]
    fn direction_mode_defaults_to_rts_software_when_both_unset() {
        let cfg = from_toml(
            r#"
            [device]
            id = "dev"
            [serial]
            kind = "rs485"
            port = "/dev/ttyAMA0"
            "#,
        );
        let serial = cfg.serial.as_ref().expect("serial present");
        let mode = serial
            .direction_mode(cfg.direction)
            .expect("rts never fails to map");
        match mode {
            DirectionMode::RtsSoftware {
                drain,
                settle_tx_us,
                settle_rx_us,
            } => {
                assert!(drain, "default must drain TX before dropping DE");
                assert_eq!(settle_tx_us, 5_000);
                assert_eq!(settle_rx_us, 10_000);
            }
            other => panic!("expected the RtsSoftware default, got {other:?}"),
        }
    }

    #[test]
    fn direction_mode_gpio_without_chip_or_line_is_a_config_error() {
        let cfg = from_toml(
            r#"
            [device]
            id = "dev"
            [serial]
            kind = "rs485"
            port = "/dev/ttyAMA0"
            direction = "gpio"
            "#,
        );
        let serial = cfg.serial.as_ref().expect("serial present");
        let result = serial.direction_mode(cfg.direction);
        assert!(
            result.is_err(),
            "direction = \"gpio\" without gpio_chip/gpio_line must fail config load, not \
             silently fall back"
        );
    }

    #[test]
    fn direction_mode_threads_settle_and_tx_drain_values_through() {
        let cfg = from_toml(
            r#"
            [device]
            id = "dev"
            [serial]
            kind = "rs485"
            port = "/dev/ttyAMA0"
            direction = "rts"
            settle_tx_us = 1234
            settle_rx_us = 5678
            tx_drain = false
            "#,
        );
        let serial = cfg.serial.as_ref().expect("serial present");
        let mode = serial
            .direction_mode(cfg.direction)
            .expect("rts never fails to map");
        match mode {
            DirectionMode::RtsSoftware {
                drain,
                settle_tx_us,
                settle_rx_us,
            } => {
                assert!(
                    !drain,
                    "tx_drain = false must thread through as drain: false"
                );
                assert_eq!(settle_tx_us, 1234);
                assert_eq!(settle_rx_us, 5678);
            }
            other => panic!("expected RtsSoftware, got {other:?}"),
        }
    }

    #[test]
    fn direction_mode_threads_gpio_active_high_and_settle_values_through() {
        let cfg = from_toml(
            r#"
            [device]
            id = "dev"
            [serial]
            kind = "rs485"
            port = "/dev/ttyAMA0"
            direction = "gpio"
            gpio_chip = "gpiochip4"
            gpio_line = 7
            gpio_active_high = false
            settle_tx_us = 111
            settle_rx_us = 222
            "#,
        );
        let serial = cfg.serial.as_ref().expect("serial present");
        let mode = serial
            .direction_mode(cfg.direction)
            .expect("gpio_chip/gpio_line both present");
        match mode {
            DirectionMode::GpioDe {
                chip,
                line,
                active_high,
                settle_tx_us,
                settle_rx_us,
            } => {
                assert_eq!(chip, "gpiochip4");
                assert_eq!(line, 7);
                assert!(!active_high);
                assert_eq!(settle_tx_us, 111);
                assert_eq!(settle_rx_us, 222);
            }
            other => panic!("expected GpioDe, got {other:?}"),
        }
    }
}

//! Renders mosquitto bridge blocks from typed figment config sections — the single source of
//! truth for both the private-cloud bridge (`[cloud]`, `connection cloud-relay`, Bridge 2) and the
//! AWS Greengrass backwards-compat bridge (`[greengrass]`, `connection greengrass-relay`, Bridge
//! 3, Scope D — closes #52-#54).
//!
//! mosquitto 2.x does not expand `${...}` environment variables inside its own config file, so a
//! static `distro/deploy/mosquitto.conf` with placeholders can never actually be filled in by mosquitto
//! itself. Instead this module renders real, placeholder-free config fragments to disk that
//! `distro/deploy/mosquitto.conf`'s `include_dir` picks up. [`render_bridge`] fails closed on a missing
//! non-secret field or the `CLOUD_MQTT_PASS` secret; [`render_greengrass_bridge`] fails closed the
//! same way on a missing mTLS field (it has no secret env var — the Greengrass core authenticates
//! this device via its own client certificate, not a password).

use crate::config::{CloudConfig, GreengrassConfig};
use std::io::Write as _;
use std::path::Path;

/// The only environment variable the password may come from. Never a config field.
pub const CLOUD_PASSWORD_ENV: &str = "CLOUD_MQTT_PASS";

/// Render the `connection cloud-relay` block.
///
/// - `Ok(None)` — no `[cloud]` config at all: a local-only device. Not an error.
/// - `Ok(Some(text))` — the rendered block, ready to write to disk.
/// - `Err(_)` — `[cloud]` is present but a required non-secret field or the password secret is
///   missing/empty. Fail closed: never emit a bridge with an empty credential.
pub fn render_bridge(
    cloud: Option<&CloudConfig>,
    password: Option<&str>,
) -> anyhow::Result<Option<String>> {
    let cloud = match cloud {
        None => return Ok(None),
        Some(c) => c,
    };

    if cloud.host.trim().is_empty() {
        anyhow::bail!("[cloud] config is present but host is empty; refusing to render a bridge");
    }
    if cloud.username.trim().is_empty() {
        anyhow::bail!(
            "[cloud] config is present but username is empty; refusing to render a bridge"
        );
    }
    if cloud.ca_file.trim().is_empty() {
        anyhow::bail!(
            "[cloud] config is present but ca_file is empty; refusing to render a bridge"
        );
    }
    let password = password
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .ok_or_else(|| {
            anyhow::anyhow!(
            "[cloud] config is present but {CLOUD_PASSWORD_ENV} is unset/empty; refusing to emit a \
             cloud-relay bridge with no credential (fail closed — see docs/deployment.md)"
        )
        })?;

    let mut out = String::new();
    out.push_str("# --- Rendered by `bin-rpi --emit-mosquitto-bridge` — DO NOT EDIT BY HAND ---\n");
    out.push_str("# Regenerated on every mosquitto (re)start from the [cloud] config + CLOUD_MQTT_PASS secret.\n");
    out.push_str("connection cloud-relay\n");
    out.push_str(&format!("address {}:{}\n", cloud.host, cloud.port));
    out.push_str(&format!("remote_username {}\n", cloud.username));
    out.push_str(&format!("remote_password {password}\n"));
    out.push_str(&format!("bridge_cafile {}\n", cloud.ca_file));
    if let Some(id) = &cloud.remote_clientid {
        out.push_str(&format!("remote_clientid {id}\n"));
    }
    out.push_str("bridge_insecure false\n");
    out.push_str(&format!("topic {} out {}\n", cloud.topic, cloud.qos));
    out.push_str("cleansession false\n");
    out.push_str("bridge_protocol_version mqttv311\n");
    out.push_str("restart_timeout 5 30\n");

    Ok(Some(out))
}

/// Render the `connection greengrass-relay` block (mosquitto Bridge 3 in
/// `distro/deploy/mosquitto.conf`) from the figment `[greengrass]` config — the AWS Greengrass
/// backwards-compat route (Scope D, closes #52-#54). The Greengrass core authenticates this
/// device via mTLS (this device's own X.509 client certificate/key), not a username/password, so
/// — unlike [`render_bridge`] — there is no secret environment variable to read: `key_file` names
/// a path mosquitto reads directly off disk; the key material itself never passes through this
/// process's memory as a string.
///
/// - `Ok(None)` — no `[greengrass]` config at all: this device never bridges to Greengrass. Not
///   an error.
/// - `Ok(Some(text))` — the rendered block, ready to write to disk.
/// - `Err(_)` — `[greengrass]` is present but a required mTLS field is empty. Fail closed, same as
///   [`render_bridge`]: never emit a bridge with a missing certificate/key/CA path.
pub fn render_greengrass_bridge(
    greengrass: Option<&GreengrassConfig>,
) -> anyhow::Result<Option<String>> {
    let gg = match greengrass {
        None => return Ok(None),
        Some(g) => g,
    };

    if gg.host.trim().is_empty() {
        anyhow::bail!(
            "[greengrass] config is present but host is empty; refusing to render a bridge"
        );
    }
    if gg.cert_file.trim().is_empty() {
        anyhow::bail!(
            "[greengrass] config is present but cert_file is empty; refusing to render a bridge"
        );
    }
    if gg.key_file.trim().is_empty() {
        anyhow::bail!(
            "[greengrass] config is present but key_file is empty; refusing to render a bridge"
        );
    }
    if gg.ca_file.trim().is_empty() {
        anyhow::bail!(
            "[greengrass] config is present but ca_file is empty; refusing to render a bridge"
        );
    }

    let mut out = String::new();
    out.push_str("# --- Rendered by `bin-rpi --emit-mosquitto-bridge` — DO NOT EDIT BY HAND ---\n");
    out.push_str(
        "# Regenerated on every mosquitto (re)start from the [greengrass] config (mTLS — no secret env var).\n",
    );
    out.push_str("connection greengrass-relay\n");
    out.push_str(&format!("address {}:{}\n", gg.host, gg.port));
    out.push_str(&format!("bridge_cafile {}\n", gg.ca_file));
    out.push_str(&format!("bridge_certfile {}\n", gg.cert_file));
    out.push_str(&format!("bridge_keyfile {}\n", gg.key_file));
    if let Some(id) = &gg.remote_clientid {
        out.push_str(&format!("remote_clientid {id}\n"));
    }
    out.push_str("bridge_insecure false\n");
    // mosquitto bridge topic remap: `topic <pattern> <direction> <qos> <local-prefix>
    // <remote-prefix>` — an empty local-prefix ("") plus a remote-prefix remaps
    // `telemetry/<device_type>/<device_id>` to `<remote_topic_prefix>telemetry/<device_type>/<device_id>`
    // on the Greengrass side. PROVISIONAL: the real AWS Greengrass core inbound topic namespace this should target
    // is unconfirmed (see `GreengrassConfig`'s doc comment) — `remote_topic_prefix` defaults empty
    // (no remap) until it is.
    out.push_str(&format!(
        "topic {} out {} \"\" {}\n",
        gg.local_topic, gg.qos, gg.remote_topic_prefix
    ));
    out.push_str("cleansession false\n");
    out.push_str("bridge_protocol_version mqttv311\n");
    out.push_str("restart_timeout 5 30\n");

    Ok(Some(out))
}

/// Read the password secret from the environment. The only place this project ever reads
/// `CLOUD_MQTT_PASS` — kept as a one-line seam so the rest of the render path stays pure/testable.
pub fn read_password_secret() -> Option<String> {
    std::env::var(CLOUD_PASSWORD_ENV).ok()
}

/// Write a rendered bridge ([`render_bridge`] or [`render_greengrass_bridge`]'s output) to
/// `out_path`, or (on `None`) remove any stale rendered file so a device that has dropped the
/// corresponding config section doesn't keep bridging with a leftover file. Creates parent
/// directories if needed. Sets 0600 perms on the written file (the cloud-relay file is
/// secret-bearing — it contains the plaintext bridge password; the greengrass-relay file carries
/// no secret material itself but gets the same tight permissions defensively) — a no-op
/// permissions-wise on non-Unix targets, since this binary only ships/runs on Linux.
pub fn write_bridge_file(rendered: Option<&str>, out_path: &Path) -> anyhow::Result<()> {
    match rendered {
        None => {
            if out_path.exists() {
                std::fs::remove_file(out_path)?;
            }
            Ok(())
        }
        Some(text) => {
            if let Some(parent) = out_path.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent)?;
                }
            }
            let mut f = std::fs::File::create(out_path)?;
            f.write_all(text.as_bytes())?;
            set_secret_permissions(out_path)?;
            Ok(())
        }
    }
}

#[cfg(unix)]
fn set_secret_permissions(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_secret_permissions(_path: &Path) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(test)]
// Test code may unwrap/panic; the workspace denies these in logic only.
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn fixture_cloud() -> CloudConfig {
        toml_cloud(
            r#"
            host = "mqtt.sentient.example"
            port = 8883
            username = "meteor-001"
            ca_file = "/etc/sentient/cloud-ca.pem"
            remote_clientid = "meteor-001-bridge"
            "#,
        )
    }

    fn toml_cloud(body: &str) -> CloudConfig {
        use figment::providers::{Format, Toml};
        use figment::Figment;
        #[derive(serde::Deserialize)]
        struct Wrap {
            cloud: CloudConfig,
        }
        let wrapped = format!("[cloud]\n{body}");
        let w: Wrap = Figment::new()
            .merge(Toml::string(&wrapped))
            .extract()
            .expect("parse fixture cloud toml");
        w.cloud
    }

    #[test]
    fn no_cloud_config_renders_nothing() {
        let rendered = render_bridge(None, Some("irrelevant")).expect("ok");
        assert!(rendered.is_none());
    }

    #[test]
    fn renders_expected_bridge_block_with_password_from_env_fixture_not_toml() {
        let cloud = fixture_cloud();
        // The password fixture stands in for CLOUD_MQTT_PASS — deliberately NOT present anywhere
        // in the TOML-sourced `cloud` value above.
        let rendered = render_bridge(Some(&cloud), Some("s3cret-from-env-fixture"))
            .expect("ok")
            .expect("some");

        assert!(rendered.contains("connection cloud-relay"));
        assert!(rendered.contains("address mqtt.sentient.example:8883"));
        assert!(rendered.contains("remote_username meteor-001"));
        assert!(rendered.contains("remote_password s3cret-from-env-fixture"));
        assert!(rendered.contains("bridge_cafile /etc/sentient/cloud-ca.pem"));
        assert!(rendered.contains("remote_clientid meteor-001-bridge"));
        assert!(rendered.contains("topic tms/# out 1"));
        assert!(rendered.contains("bridge_insecure false"));
        assert!(!rendered.contains("${"));
    }

    #[test]
    fn fails_closed_when_password_env_is_unset() {
        let cloud = fixture_cloud();
        let err = render_bridge(Some(&cloud), None).expect_err("must fail closed");
        assert!(err.to_string().contains(CLOUD_PASSWORD_ENV));
    }

    #[test]
    fn fails_closed_when_password_env_is_empty() {
        let cloud = fixture_cloud();
        let err = render_bridge(Some(&cloud), Some("")).expect_err("must fail closed");
        assert!(err.to_string().contains(CLOUD_PASSWORD_ENV));
    }

    #[test]
    fn fails_closed_when_host_missing() {
        let cloud = toml_cloud(
            r#"
            host = ""
            username = "u"
            ca_file = "/ca.pem"
            "#,
        );
        let err = render_bridge(Some(&cloud), Some("pw")).expect_err("must fail closed");
        assert!(err.to_string().contains("host"));
    }

    // ---- render_greengrass_bridge (Scope D, #52-#54) ------------------------------------------

    fn fixture_greengrass() -> GreengrassConfig {
        toml_greengrass(
            r#"
            host = "gg-core.local"
            cert_file = "/etc/sentient/gg-cert.pem"
            key_file = "/etc/sentient/gg-key.pem"
            ca_file = "/etc/sentient/gg-ca.pem"
            remote_clientid = "meteor-001-greengrass"
            "#,
        )
    }

    fn toml_greengrass(body: &str) -> GreengrassConfig {
        use figment::providers::{Format, Toml};
        use figment::Figment;
        #[derive(serde::Deserialize)]
        struct Wrap {
            greengrass: GreengrassConfig,
        }
        let wrapped = format!("[greengrass]\n{body}");
        let w: Wrap = Figment::new()
            .merge(Toml::string(&wrapped))
            .extract()
            .expect("parse fixture greengrass toml");
        w.greengrass
    }

    #[test]
    fn no_greengrass_config_renders_nothing() {
        let rendered = render_greengrass_bridge(None).expect("ok");
        assert!(rendered.is_none());
    }

    #[test]
    fn renders_expected_greengrass_bridge_block() {
        let gg = fixture_greengrass();
        let rendered = render_greengrass_bridge(Some(&gg))
            .expect("ok")
            .expect("some");

        assert!(rendered.contains("connection greengrass-relay"));
        assert!(rendered.contains("address gg-core.local:8883"));
        assert!(rendered.contains("bridge_cafile /etc/sentient/gg-ca.pem"));
        assert!(rendered.contains("bridge_certfile /etc/sentient/gg-cert.pem"));
        assert!(rendered.contains("bridge_keyfile /etc/sentient/gg-key.pem"));
        assert!(rendered.contains("remote_clientid meteor-001-greengrass"));
        // Default local_topic is now `telemetry/#` (gap-closure #269 — GreengrassTap publishes to
        // `telemetry/{device_type}/{device_id}`, no longer `legacy/{device_id}/{kind}`).
        assert!(rendered.contains("topic telemetry/# out 1"));
        assert!(rendered.contains("bridge_insecure false"));
        // No password/username anywhere: mTLS auth only.
        assert!(!rendered.contains("remote_username"));
        assert!(!rendered.contains("remote_password"));
        assert!(!rendered.contains("${"));
    }

    #[test]
    fn greengrass_bridge_honors_configured_topic_remap() {
        let gg = toml_greengrass(
            r#"
            host = "gg-core.local"
            cert_file = "/c.pem"
            key_file = "/k.pem"
            ca_file = "/ca.pem"
            local_topic = "telemetry/battery/#"
            remote_topic_prefix = "meteor/"
            qos = 0
            "#,
        );
        let rendered = render_greengrass_bridge(Some(&gg))
            .expect("ok")
            .expect("some");
        assert!(rendered.contains("topic telemetry/battery/# out 0 \"\" meteor/"));
    }

    #[test]
    fn greengrass_bridge_fails_closed_when_host_missing() {
        let gg = toml_greengrass(
            r#"
            host = ""
            cert_file = "/c.pem"
            key_file = "/k.pem"
            ca_file = "/ca.pem"
            "#,
        );
        let err = render_greengrass_bridge(Some(&gg)).expect_err("must fail closed");
        assert!(err.to_string().contains("host"));
    }

    #[test]
    fn greengrass_bridge_fails_closed_when_cert_file_missing() {
        let gg = toml_greengrass(
            r#"
            host = "gg-core.local"
            cert_file = ""
            key_file = "/k.pem"
            ca_file = "/ca.pem"
            "#,
        );
        let err = render_greengrass_bridge(Some(&gg)).expect_err("must fail closed");
        assert!(err.to_string().contains("cert_file"));
    }

    #[test]
    fn greengrass_bridge_fails_closed_when_key_file_missing() {
        let gg = toml_greengrass(
            r#"
            host = "gg-core.local"
            cert_file = "/c.pem"
            key_file = ""
            ca_file = "/ca.pem"
            "#,
        );
        let err = render_greengrass_bridge(Some(&gg)).expect_err("must fail closed");
        assert!(err.to_string().contains("key_file"));
    }

    #[test]
    fn greengrass_bridge_fails_closed_when_ca_file_missing() {
        let gg = toml_greengrass(
            r#"
            host = "gg-core.local"
            cert_file = "/c.pem"
            key_file = "/k.pem"
            ca_file = ""
            "#,
        );
        let err = render_greengrass_bridge(Some(&gg)).expect_err("must fail closed");
        assert!(err.to_string().contains("ca_file"));
    }

    #[test]
    fn write_bridge_file_works_for_the_greengrass_relay_file_too() {
        let dir = std::env::temp_dir().join(format!("bin-rpi-test-gg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let out = dir.join("greengrass-relay.conf");

        write_bridge_file(Some("connection greengrass-relay\n"), &out).expect("write");
        assert_eq!(
            std::fs::read_to_string(&out).expect("read"),
            "connection greengrass-relay\n"
        );

        write_bridge_file(None, &out).expect("remove");
        assert!(!out.exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    // Mutates the process-global `CLOUD_MQTT_PASS` env — serialize against
    // `config::tests::debug_output_never_contains_the_password`, the other test that touches it.
    #[serial_test::serial(cloud_mqtt_pass_env)]
    fn read_password_secret_reads_the_documented_env_var() {
        // Serialized within a single test to avoid racing other tests over process-global env.
        std::env::remove_var(CLOUD_PASSWORD_ENV);
        assert!(read_password_secret().is_none());
        std::env::set_var(CLOUD_PASSWORD_ENV, "fixture-pw");
        assert_eq!(read_password_secret().as_deref(), Some("fixture-pw"));
        std::env::remove_var(CLOUD_PASSWORD_ENV);
    }

    #[test]
    fn write_bridge_file_sets_0600_and_removes_stale_file_on_none() {
        let dir = std::env::temp_dir().join(format!("bin-rpi-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let out = dir.join("cloud-relay.conf");

        write_bridge_file(Some("connection cloud-relay\n"), &out).expect("write");
        assert!(out.exists());
        assert_eq!(
            std::fs::read_to_string(&out).expect("read"),
            "connection cloud-relay\n"
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&out).expect("meta").permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }

        write_bridge_file(None, &out).expect("remove");
        assert!(!out.exists());

        let _ = std::fs::remove_dir_all(&dir);
    }
}

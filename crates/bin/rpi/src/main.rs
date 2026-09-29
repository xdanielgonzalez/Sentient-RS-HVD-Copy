//! `bin-rpi` — the Raspberry Pi comms-module binary (the contract deliverable).
//!
//! This crate is the ports-and-adapters **composition root** for the RPi target (ADR 0009,
//! `docs/decisions/0009-spec-app-ports-and-adapters.md`): it is the one place that knows about all
//! three layers at once — the contract (`spec-app`: `RunConfig`/`MAX_DIPS`/`preflight::Preflight`),
//! the policy (`impl_app::{run, run_inverter}`, the shared Seplos/Voltronic poll loops), and the
//! adapter (`bin_rpi_platform::HostPlatform`, the tokio/rumqttd platform impl of the `Platform`
//! seam). A thin CLI dispatcher: load `Config` (TOML + `SENTIENT_` env), then hand off to
//! [`runtime::supervise`] (build one `HostPlatform` per configured bus and hand it to the shared
//! poll loop) — or, for `--emit-mosquitto-bridge`, render the cloud-relay bridge fragment (and,
//! alongside it, the Greengrass legacy-relay bridge fragment — Scope D, closes #52-#54) and exit.
//! The pipeline is identical to the Zephyr build — only the platform adapter differs
//! (`bin-rpi-platform` here, `bin-zephyr-platform` there); no feature flag on `impl-app` selects it
//! any more (that crate no longer has a `host` feature — see its crate doc). An external mosquitto
//! bridge relays the broker's TMS topics to the cloud, and legacy topics to the AWS Greengrass
//! core, (see distro/deploy/mosquitto.conf).
//!
//! Split from a single file into `main.rs` (dispatcher) + `runtime.rs` (poll-loop supervision) +
//! `scan.rs` (bus scanner stub) + `tasks/` (heartbeat/reply-listener stubs) as gap-closure
//! scaffolding (#237) — see `docs/gap-analysis-meteor-parity.md`. `runtime::supervise` preserves
//! the exact prior single-port behavior; only the split itself is new here.

#![forbid(unsafe_code)]

mod config;
mod mosquitto_bridge;
mod runtime;
mod scan;
mod tasks;

use config::Config;

/// Locate the config: `SENTIENT_CONFIG`, then a couple of conventional paths, else env-only.
fn config_path() -> Option<String> {
    if let Ok(p) = std::env::var("SENTIENT_CONFIG") {
        return Some(p);
    }
    ["config/sentient.toml", "sentient.toml"]
        .into_iter()
        .find(|c| std::path::Path::new(c).exists())
        .map(str::to_string)
}

/// Render the mosquitto `connection cloud-relay` bridge fragment from the figment `[cloud]`
/// config + the `CLOUD_MQTT_PASS` secret env var, and write it to `out_path` (0600). No `[cloud]`
/// config means "local-only device" — not an error; any stale rendered file at `out_path` is
/// removed instead. This is the single render path: `distro/deploy/mosquitto-override.conf` runs
/// `bin-rpi --emit-mosquitto-bridge <out>` as an `ExecStartPre` before mosquitto starts.
///
/// Also renders mosquitto Bridge 3 (Scope D, closes #52-#54): the AWS Greengrass mTLS relay from
/// the figment `[greengrass]` config, via [`mosquitto_bridge::render_greengrass_bridge`]. It is
/// written to a sibling file in `out_path`'s parent directory (`greengrass-relay.conf`) rather
/// than a second CLI argument — both files land in the same mosquitto `include_dir`
/// (`distro/deploy/mosquitto.conf`'s header), so a single `--emit-mosquitto-bridge <out>`
/// invocation (unchanged call site in `distro/deploy/mosquitto-override.conf` /
/// `distro/meta-sentient/.../mosquitto-override.conf`) renders both bridges. No `[greengrass]`
/// config means this device never bridges to Greengrass — not an error; any stale rendered file is
/// removed the same way as the cloud bridge.
fn emit_mosquitto_bridge(out_path: &str) -> anyhow::Result<()> {
    let cfg = Config::load(config_path().as_deref())?;
    if let Some(cloud) = cfg.cloud() {
        // Non-secret only: the CLOUD_MQTT_* values this render draws from (never the password).
        log::info!("rendering cloud-relay bridge from {:?}", cloud.env_map());
    }
    let password = mosquitto_bridge::read_password_secret();
    let rendered = mosquitto_bridge::render_bridge(cfg.cloud(), password.as_deref())?;
    let path = std::path::Path::new(out_path);
    mosquitto_bridge::write_bridge_file(rendered.as_deref(), path)?;
    match rendered {
        Some(_) => log::info!("mosquitto cloud-relay bridge rendered to {out_path}"),
        None => log::info!(
            "no [cloud] config; local-only device — no cloud-relay bridge emitted (removed any stale file at {out_path})"
        ),
    }

    let gg_path = greengrass_bridge_out_path(path);
    let gg_rendered = mosquitto_bridge::render_greengrass_bridge(cfg.greengrass.as_ref())?;
    mosquitto_bridge::write_bridge_file(gg_rendered.as_deref(), &gg_path)?;
    match gg_rendered {
        Some(_) => log::info!(
            "mosquitto greengrass-relay bridge rendered to {}",
            gg_path.display()
        ),
        None => log::info!(
            "no [greengrass] config; no greengrass-relay bridge emitted (removed any stale file at {})",
            gg_path.display()
        ),
    }

    Ok(())
}

/// Derive the greengrass-relay sibling output path from the cloud-relay `out_path`: same parent
/// directory (mosquitto's rendered-bridge `include_dir`), filename `greengrass-relay.conf`. Falls
/// back to a bare relative filename if `out_path` has no parent component (e.g. a bare filename
/// passed with no directory).
fn greengrass_bridge_out_path(cloud_out_path: &std::path::Path) -> std::path::PathBuf {
    match cloud_out_path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.join("greengrass-relay.conf"),
        _ => std::path::PathBuf::from("greengrass-relay.conf"),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // `--emit-mosquitto-bridge <out>`: render the cloud-relay bridge fragment and exit. No tokio
    // runtime work needed for this path, but it shares `main`'s logging setup.
    let args: Vec<String> = std::env::args().collect();
    if let Some(idx) = args.iter().position(|a| a == "--emit-mosquitto-bridge") {
        let out_path = args.get(idx + 1).ok_or_else(|| {
            anyhow::anyhow!("--emit-mosquitto-bridge requires an output path argument")
        })?;
        return emit_mosquitto_bridge(out_path);
    }

    // `--scan-bus [--port P] [--baud N] [--timeout-ms N]`: standalone DIP-address bus scanner
    // field tool (#243) — enter scan mode and exit, never reaching `runtime::supervise`. See
    // `crate::scan`'s module doc.
    if let Some(scan_args) = scan::ScanArgs::parse(&args) {
        return scan::run(&scan_args).await;
    }

    let cfg = Config::load(config_path().as_deref())?;
    runtime::supervise(cfg).await
}

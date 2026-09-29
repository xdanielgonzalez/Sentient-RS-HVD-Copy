//! Poll-loop supervision (#238 fan-out, #242 multi-port): builds one [`HostPlatform`] per
//! configured serial bus sharing a single embedded broker, spawns each bus's poll loop plus the
//! shared secondary-publish background tasks (disk writer, mesh radio, location refresher,
//! heartbeat, reply listener), aggregates every bus/task's liveness into one systemd watchdog
//! feed via [`WatchdogAgg`], and drives a bounded, ordered graceful shutdown.
//!
//! Design (locked, see the umbrella story #237's WS1 handoff): the embedded broker is owned HERE
//! (not inside `HostPlatform` — see that type's doc comment), which mints one `BrokerPublisher`
//! per bus (cheap — shares the broker's `LinkTx` via `Arc`). Multi-port = one
//! `impl_app::run(HostPlatform, RunConfig)` tokio task per configured bus; fan-out = each bus's
//! `HostPlatform`-internal `SinkSet` of taps, sharing the process-wide disk/mesh background-task
//! handles (constructed once, cloned into every bus that needs them) for per-target isolation.
//!
//! **Composition-root judgement (ADR 0009, flagged, kept as-is):** the [`LocalBroker`] lifecycle
//! (construction/start/shutdown) and per-bus [`BrokerPublisher`] minting deliberately STAY here —
//! they do NOT move into `bin-rpi-platform`. The broker is one process-wide resource shared by N
//! per-bus `HostPlatform`s (the design paragraph above), and ADR 0009 explicitly confines
//! concretion knowledge to the composition roots ("something must link the broker"). Consequence:
//! `bin-rpi` legitimately keeps a normal `lib-mqtt` dependency (`LocalBroker` here + the loopback
//! `rumqttc` heartbeat/reply-listener/mc-select tasks in `tasks/`), and hands each minted
//! `BrokerPublisher` into [`bin_rpi_platform::HostPlatform::build_port`] below — the adapter never
//! constructs or owns a broker itself.

use std::collections::HashMap;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::config::{BusConfig, Config, DdsToml, IdentityCfg, ProductParams, ProtocolCfg};
use crate::tasks::{
    dds::heartbeat::run_dds_heartbeat, dds::request_listener::run_dds_request_listener,
    dds::state_publisher::run_dds_state_publisher, heartbeat::run_heartbeat,
    mc_select::run_mc_select, reply_listener::run_reply_listener,
    state_publisher::run_state_publisher,
};
// The adapter (ADR 0009): the RPi platform impl of the app's seams.
use bin_rpi_platform::{
    notify_ready, notify_stopping, CaptureHandle, CloudCheckInfo, DdsTap, DiskTap, GreengrassTap,
    HostPlatform, HostTap, MeshTap, PreflightInputs, SinkSet, TimingProfile, WatchdogAgg,
};
// The policy: the shared Seplos/Voltronic poll loops. Named directly from impl-app (not the
// contract) because these ARE impl-app's own entry points.
use impl_app::{run, run_inverter};
use lib_dds::{dds_channel, run_dds_stack, DdsConfig, DdsHandle};
use lib_io::persist::JsonlHandle;
use lib_mapping::schema::Identity;
use lib_mqtt::broker::{BrokerPublisher, LocalBroker};
use lib_net::mesh::RadioHandle;
// The contract (ADR 0009): named straight from spec-app rather than via impl-app's
// path-stability re-exports, so this composition root reads as contract + policy + adapter.
use spec_app::preflight::Preflight;
use spec_app::{RunConfig, MAX_DIPS};
use tokio::task::JoinSet;

/// How stale a registered `CyclePulse` may go before the aggregated systemd watchdog feed
/// withholds `WATCHDOG=1` — generous relative to any expected poll cadence so normal operation
/// never trips it, but tight enough to catch a genuinely wedged bus or background task.
const WATCHDOG_WINDOW_MS: u64 = 60_000;
/// Bounded queue depth between a bus's poll loop and the shared mesh-radio task (no per-config
/// knob for this yet — see `MeshtasticConfig`).
const MESH_QUEUE_DEPTH: usize = 32;
/// Bounded budget for draining every background task after shutdown is signaled.
const DRAIN_TIMEOUT: Duration = Duration::from_secs(5);

/// Truncate `s` to the first `max` chars and push into a bounded heapless string.
fn bounded_str<const N: usize>(s: &str, max: usize) -> heapless::String<N> {
    let end = s.char_indices().nth(max).map_or(s.len(), |(i, _)| i);
    let mut out = heapless::String::new();
    let _ = out.push_str(&s[..end.min(s.len())]);
    out
}

/// Build the platform-neutral run parameters for one bus from the parsed config, stamping this
/// bus's derived [`IdentityCfg`] `device_id` (D13) — NOT the single global `[device].id` — so each
/// bus's TMS payloads carry its own identity (`{device.id}` for the battery bus, `{device.id}-inv`
/// for the inverter bus, or an explicit `[[bus]].device_id`).
fn run_config(cfg: &Config, bus: &BusConfig, identity: &IdentityCfg) -> RunConfig {
    let mut dips = heapless::Vec::<u8, MAX_DIPS>::new();
    for &d in &bus.poll.dip_addresses {
        if dips.push(d).is_err() {
            log::warn!(
                "bus {}: more than {MAX_DIPS} dip addresses configured; extra ignored",
                bus.name
            );
            break;
        }
    }
    RunConfig {
        device_id: bounded_str::<32>(&identity.device_id, 32),
        prefix: bounded_str::<16>(&cfg.publish.prefix, 16),
        dip_addresses: dips,
        interval_ms: bus.poll.interval_ms as u32,
        fault_every: bus.poll.fault_every,
        // No per-config knob for this yet (like `MESH_QUEUE_DEPTH` above): the Pi keeps the
        // existing 1 s -> 30 s dead-cycle ramp — a truly-absent BMS shouldn't be hammered.
        max_backoff_ms: impl_app::Backoff::MAX_MS,
    }
}

/// Build the `lib_dds::DdsConfig` for `[dds]` (#324, WP6): domain-0-shaped defaults (SPDP group,
/// loopback unicast advertisement, queue depth — [`DdsConfig::domain0`]) with `domain_id`/`iface`/
/// `unicast_addr` overridden from the TOML; `iface` falls back to `"eth0"` when omitted
/// (`DdsToml::iface`'s doc comment). A malformed `unicast_addr` literal is a hard config error (the
/// same fail-loudly posture `Config::identities`/`buses` already established), not a silent
/// fallback to the loopback default. `DdsToml::validate` is called first, so a non-zero
/// `domain_id` (MIL-STD-3071 §D.2 mandates domain 0) also fails loudly here rather than being
/// silently honored.
fn dds_config(cfg: &DdsToml) -> anyhow::Result<DdsConfig> {
    cfg.validate()?;
    let iface = cfg.iface.clone().unwrap_or_else(|| "eth0".to_string());
    let mut config = DdsConfig::domain0(iface);
    config.domain_id = cfg.domain_id;
    if let Some(addr) = &cfg.unicast_addr {
        config.unicast_addr = Some(addr.parse().map_err(|e| {
            anyhow::anyhow!(
                "config error: [dds].unicast_addr `{addr}` is not a valid IPv6 literal: {e}"
            )
        })?);
    }
    Ok(config)
}

/// Gather the non-secret inputs the host's preflight checks need for one bus, mirroring the exact
/// fail-closed conditions `mosquitto_bridge::render_bridge` checks (so `cloud_creds`/
/// `mosquitto_bridge` preflight results agree with what `--emit-mosquitto-bridge` would actually
/// do), without handing the checks the config types themselves.
fn preflight_inputs(cfg: &Config, bus: &BusConfig) -> PreflightInputs {
    let cloud = cfg.cloud().map(|c| {
        let password_present = crate::mosquitto_bridge::read_password_secret()
            .as_deref()
            .map(str::trim)
            .is_some_and(|p| !p.is_empty());
        CloudCheckInfo {
            host_present: !c.host.trim().is_empty(),
            username_present: !c.username.trim().is_empty(),
            ca_file_present: !c.ca_file.trim().is_empty(),
            password_present,
        }
    });
    PreflightInputs {
        device_id_present: !cfg.device.id.trim().is_empty(),
        dip_addresses_present: !bus.poll.dip_addresses.is_empty(),
        cloud,
    }
}

/// Wait for SIGTERM. Unix only (the Pi's shipping target — see CLAUDE.md "Platform &
/// Deployment"); on any other host this simply never resolves, leaving Ctrl-C/task-exit as the
/// shutdown triggers.
#[cfg(unix)]
async fn wait_for_sigterm() {
    use tokio::signal::unix::{signal, SignalKind};
    match signal(SignalKind::terminate()) {
        Ok(mut sig) => {
            sig.recv().await;
        }
        Err(e) => {
            log::warn!("failed to install SIGTERM handler: {e}");
            std::future::pending::<()>().await;
        }
    }
}
#[cfg(not(unix))]
async fn wait_for_sigterm() {
    std::future::pending::<()>().await;
}

/// Build the secondary-publish taps enabled for one bus, sharing the process-wide disk/mesh
/// background-task handles (constructed once by [`supervise`], cloned in here) — each bus gets
/// its own [`SinkSet`] but the same underlying writer/radio task.
///
/// `greengrass` is `Some((publisher, device_id, device_type, location_rx))` when `[greengrass]` is
/// configured: its own freshly minted [`BrokerPublisher`] handle (never shared with the bus's
/// primary publisher — see [`GreengrassTap`]'s doc comment), this bus's DERIVED identity device_id
/// and its `device_type` (`"battery"` for a Seplos bus, `"inverter"` for a Voltronic bus — D13; see
/// [`IdentityCfg::device_type`]), and a clone of the process-wide location watch receiver.
///
/// `mesh_handle` is `Some((handle, max_payload, location_rx))` when `[meshtastic]` is configured
/// (#387): a clone of the process-wide [`RadioHandle`], the configured on-air budget, and a clone
/// of the process-wide location watch receiver — mirrors `greengrass`'s `location_rx` exactly (same
/// fallback-to-an-always-`None`-channel discipline at the call site) so a mesh frame can carry a
/// location tag whenever one is available. `MeshTap` gets its own fresh per-DIP reading cache here
/// (see [`MeshTap::last_reading`]'s doc comment) — never shared across buses.
///
/// `dds` is `Some((handle, device_id))` when `[dds]` is configured (#324, WP6): a clone of the
/// process-wide [`DdsHandle`] (shared with `run_dds_stack` and every `tasks::dds::*` background
/// task) plus this bus's derived identity device_id — see [`DdsTap`]'s doc comment.
fn build_taps(
    disk_handle: Option<&JsonlHandle>,
    mesh_handle: Option<(
        &RadioHandle,
        usize,
        tokio::sync::watch::Receiver<Option<lib_net::locate::LocationFix>>,
    )>,
    greengrass: Option<(
        BrokerPublisher,
        &str,
        &str,
        tokio::sync::watch::Receiver<Option<lib_net::locate::LocationFix>>,
    )>,
    dds: Option<(&DdsHandle, &str)>,
) -> SinkSet {
    let mut taps = Vec::new();
    if let Some(handle) = disk_handle {
        taps.push(HostTap::Disk(DiskTap {
            handle: handle.clone(),
        }));
    }
    if let Some((handle, max_payload, location_rx)) = mesh_handle {
        taps.push(HostTap::Mesh(MeshTap {
            handle: handle.clone(),
            max_payload,
            last_reading: Arc::new(Mutex::new(HashMap::new())),
            location_rx,
        }));
    }
    if let Some((publisher, device_id, device_type, location_rx)) = greengrass {
        taps.push(HostTap::Greengrass(GreengrassTap::new(
            publisher,
            device_id,
            device_type,
            location_rx,
        )));
    }
    if let Some((handle, device_id)) = dds {
        taps.push(HostTap::Dds(DdsTap::new(
            handle.clone(),
            bounded_str::<32>(device_id, 32),
        )));
    }
    SinkSet { taps }
}

/// Build every configured bus's host platform, run each one's preflight, and run every bus's poll
/// loop plus the shared secondary background tasks — forever, or until a graceful shutdown signal
/// (SIGTERM, Ctrl-C, or an unexpected task exit).
pub async fn supervise(cfg: Config) -> anyhow::Result<()> {
    // Resolve the process's timing profile ONCE, here, and thread it down into every consumer
    // below (each bus's `HostPlatform::build_port` + the shared `mc_select` task). Production
    // builds always get `TimingProfile::production` (the real MIL-STD-3071 spec values); the
    // TEST-ONLY compressed profile is reachable only from a debug build with
    // `SENTIENT_TEST_FAST_TIMERS` set — see `TimingProfile::resolve`'s doc comment for the full
    // debug_assertions-gated contract. Never re-resolved per-bus/per-cycle.
    let profile = TimingProfile::resolve();
    let buses = cfg.buses()?;
    // Fail loudly at startup if any bus's poll cadence would under-publish DcMeasurementUpdate
    // below its MIL-STD-3071 1 Hz requirement (see `Config::validate_poll_cadence` /
    // `PollConfig::validate`) — this is the real defect that let every shipped config regress to
    // `interval_ms = 2000` (0.5 Hz) unnoticed. `PollConfig::validate` also covers `dip_addresses`
    // (non-empty, Seplos-encodable range) here.
    Config::validate_poll_cadence(&buses)?;
    // Fail loudly on a per-bus `[serial]`/`[[bus]].serial` value that is degenerate or would
    // silently be coerced (baud=0, stop_bits outside {1,2}, read_timeout_ms=0) — see
    // `SerialConfig::validate`.
    Config::validate_bus_serial(&buses)?;
    // Fail loudly on every other config-layer semantic bound that doesn't need the resolved bus
    // list (broker/cloud/greengrass/disk/capture/meshtastic — see `Config::validate` for the full
    // list and each section's own `validate()` for the specific rule).
    cfg.validate()?;
    // One derived identity per bus (D13), in bus order — errors here (e.g. duplicate device_id
    // across buses) fail startup before the broker or any poll loop starts.
    let identities = cfg.identities()?;
    log::info!(
        "config: device={} {} bus(es); broker {}:{}",
        cfg.device.id,
        buses.len(),
        cfg.broker.host,
        cfg.broker.port,
    );
    for (bus, identity) in buses.iter().zip(identities.iter()) {
        log::info!(
            "  bus '{}' -> protocol={:?} device_id={} role={:?}",
            bus.name,
            identity.protocol,
            identity.device_id,
            identity.role,
        );
    }

    // One embedded broker for the whole process (#238): each bus mints its own publisher handle
    // off it below (cheap — shares the broker's LinkTx via Arc; see `HostPlatform`'s doc comment
    // for why the broker itself no longer lives inside `HostPlatform`).
    let mut broker = LocalBroker::start(cfg.broker_config())
        .map_err(|e| anyhow::anyhow!("broker start: {e}"))?;
    log::info!(
        "embedded MQTT server up on {}:{}",
        cfg.broker.host,
        cfg.broker.port
    );

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let mut watchdog = WatchdogAgg::default();

    // ---- Secondary background tasks: spawned once, shared across every bus's taps -------------
    let mut secondary: JoinSet<()> = JoinSet::new();

    // DDS/RTPS (MIL-STD-3071) transport (#324, WP6): only when `[dds]` is configured — MQTT stays
    // always-on regardless (transport choice stays OUT of `RunConfig`; see the DDS plan's Pi
    // integration section). `run_dds_stack` owns the participant + lazy `(device, topic)` writer
    // map, draining the SAME bounded outbound queue every `DdsTap`/`tasks::dds::*` task enqueues
    // on, spawned on this SAME secondary `JoinSet` (shutdown watch) as every other background
    // task. `dds_participant_config` is kept (not just the handle) so the per-identity DDS request
    // listener below can build its OWN participant (see `tasks::dds::request_listener`'s doc
    // comment for why it doesn't share this one).
    let dds_participant_config: Option<DdsConfig> = cfg.dds.as_ref().map(dds_config).transpose()?;
    let dds_handle: Option<DdsHandle> = dds_participant_config.clone().map(|config| {
        let (handle, rx) = dds_channel(config.queue_capacity);
        log::info!(
            "dds: enabled — domain {} iface {}",
            config.domain_id,
            config.iface
        );
        secondary.spawn(async move {
            if let Err(e) = run_dds_stack(config, rx).await {
                log::error!("dds: stack exited with error: {e}; DDS transport down for this run");
            }
        });
        handle
    });

    let disk_handle = cfg.disk.as_ref().map(|disk_cfg| {
        let policy = disk_cfg.to_policy();
        let (tx, rx) = tokio::sync::mpsc::channel(policy.queue_depth.max(1));
        let handle = JsonlHandle {
            tx,
            dropped: Arc::new(AtomicU64::new(0)),
        };
        secondary.spawn(lib_io::persist::run_writer(rx, policy, shutdown_rx.clone()));
        handle
    });

    // Raw pre-parse wire-capture writer (decision 0001's open risk — see `CaptureConfig`'s doc
    // comment): ONE writer/channel for the whole process (mirrors `disk_handle` above), threaded
    // into every configured bus's `HostPlatform::build_port` below as that bus's own
    // `CaptureHandle` (same `tx`/`dropped`, but each bus's own `bus` label — see
    // `bin_rpi_platform::CaptureSerial`). `None` when `[capture]` isn't configured, so every bus's
    // `CaptureSerial` stays a zero-overhead passthrough.
    let capture_handle_base = cfg.capture.as_ref().map(|capture_cfg| {
        let policy = capture_cfg.to_policy();
        let (tx, rx) = tokio::sync::mpsc::channel(policy.queue_depth.max(1));
        secondary.spawn(lib_io::persist::run_capture_writer(
            rx,
            policy,
            shutdown_rx.clone(),
        ));
        (tx, Arc::new(AtomicU64::new(0)))
    });

    // `[meshtastic].enabled` (default true) lets a checked-in config disable the mesh tap
    // without deleting the whole section — presence-of-section alone is no longer sufficient
    // (see `MeshtasticConfig::enabled`'s doc comment; this closes that previously-documented gap).
    let mesh_handle = cfg
        .meshtastic
        .as_ref()
        .filter(|mesh_cfg| mesh_cfg.enabled)
        .map(|mesh_cfg| {
            let mesh_config = mesh_cfg.to_mesh_config();
            let max_payload = mesh_config.max_payload;
            let (tx, rx) = tokio::sync::mpsc::channel(MESH_QUEUE_DEPTH);
            let handle = RadioHandle {
                tx,
                dropped: Arc::new(AtomicU64::new(0)),
            };
            secondary.spawn(lib_net::mesh::run_radio(
                rx,
                mesh_config,
                shutdown_rx.clone(),
            ));
            (handle, max_payload)
        });

    // Kept (not dropped) when `[location]` is configured: `GreengrassTap` (below, per bus) is the
    // consumer — a clone of this receiver tags every legacy publish with the best-effort location
    // fix. `run_refresher` ignores `tx.send`'s Result either way, so a device with no `[greengrass]`
    // tap (this receiver never cloned) is harmless — same as before this was wired.
    let location_rx = cfg.location.as_ref().map(|loc_cfg| {
        let chain = loc_cfg.to_chain();
        let (loc_tx, loc_rx) = tokio::sync::watch::channel(None);
        secondary.spawn(lib_net::locate::run_refresher(
            chain,
            loc_cfg.refresh_secs,
            loc_tx,
            shutdown_rx.clone(),
        ));
        loc_rx
    });

    // TMS-required 1 s heartbeat + the Request/Reply control-path listener: always on, not gated
    // behind an optional config section (unlike disk/mesh/location/greengrass, which are opt-in
    // secondary publish targets). Spawned ONCE PER derived identity (D13) with injected params
    // (identity + shared product info + broker host:port; no `Config` reload) — a unit running both
    // a battery and an inverter bus announces TWO DeviceInfo/Heartbeat streams and answers requests
    // on BOTH identities' topics. Identity-suffixed MQTT client-ids keep the N loopback clients from
    // colliding.
    // The MC-selection state machine (A.7.5, #264) is ONE shared task for the whole process — the
    // adapter observes the same MC Heartbeats regardless of how many buses it runs — publishing the
    // selected MC id on this `watch`. Every identity's `state_publisher` (below) takes a clone of
    // the receiver and renders it into that identity's `ActiveMicrogridControllerState`. Spawned
    // unconditionally (a telemetry-only device still owes an AMCState with `master_id = null` until
    // an MC appears — #263).
    let (mc_tx, mc_rx) = tokio::sync::watch::channel::<Option<Identity>>(None);
    secondary.spawn(run_mc_select(
        cfg.device.id.clone(),
        cfg.broker.host.clone(),
        cfg.broker.port,
        mc_tx,
        shutdown_rx.clone(),
        profile,
    ));

    let product = ProductParams::from_device(&cfg.device);
    for identity in &identities {
        secondary.spawn(run_heartbeat(
            identity.clone(),
            product.clone(),
            cfg.broker.host.clone(),
            cfg.broker.port,
            shutdown_rx.clone(),
        ));
        secondary.spawn(run_reply_listener(
            identity.clone(),
            cfg.broker.host.clone(),
            cfg.broker.port,
            shutdown_rx.clone(),
        ));
        // The four Required STOR state topics (#263), role-scoped inside the task; its AMCState
        // tracks the shared MC-selection watch.
        secondary.spawn(run_state_publisher(
            identity.clone(),
            cfg.broker.host.clone(),
            cfg.broker.port,
            mc_rx.clone(),
            shutdown_rx.clone(),
        ));

        // The DDS-transport counterparts (#324, WP6) — spawned only when `[dds]` is configured
        // (`dds_handle`/`dds_participant_config` are both `Some` together, or both `None`). Same
        // per-identity discipline as their MQTT siblings above.
        if let (Some(handle), Some(participant_config)) = (&dds_handle, &dds_participant_config) {
            secondary.spawn(run_dds_heartbeat(
                identity.clone(),
                product.clone(),
                handle.clone(),
                shutdown_rx.clone(),
            ));
            secondary.spawn(run_dds_request_listener(
                identity.clone(),
                participant_config.clone(),
                handle.clone(),
                shutdown_rx.clone(),
            ));
            secondary.spawn(run_dds_state_publisher(
                identity.clone(),
                handle.clone(),
                mc_rx.clone(),
                shutdown_rx.clone(),
            ));
        }
    }

    // ---- One HostPlatform + poll-loop task per configured bus (+ its derived identity) ----------
    let mut port_tasks: JoinSet<()> = JoinSet::new();
    for (bus, identity) in buses.iter().zip(identities.iter()) {
        let publisher = broker.publisher();
        // `GreengrassTap` needs its own broker publisher handle (never shared with `publisher`
        // above — see that type's doc comment) minted fresh per bus, plus a clone of the
        // process-wide location watch receiver (falling back to a standalone always-`None`
        // channel when `[location]` isn't configured — a device can bridge to Greengrass without
        // location tagging). This bus's identity supplies its `device_id` + `device_type`
        // (`"battery"`/`"inverter"`) so its legacy publish lands on `telemetry/{type}/{that id}` (D13).
        let greengrass = cfg.greengrass.is_some().then(|| {
            let gg_publisher = broker.publisher();
            let gg_location_rx = location_rx.clone().unwrap_or_else(|| {
                tokio::sync::watch::channel::<Option<lib_net::locate::LocationFix>>(None).1
            });
            (
                gg_publisher,
                identity.device_id.as_str(),
                identity.device_type(),
                gg_location_rx,
            )
        });
        // `MeshTap` gets the SAME location-watch treatment as `GreengrassTap` above (#387): a
        // clone of the process-wide receiver, falling back to a standalone always-`None` channel
        // when `[location]` isn't configured — a device can mesh-forward without location tagging.
        let mesh_location_rx = location_rx.clone().unwrap_or_else(|| {
            tokio::sync::watch::channel::<Option<lib_net::locate::LocationFix>>(None).1
        });
        let taps = build_taps(
            disk_handle.as_ref(),
            mesh_handle.as_ref().map(|(h, m)| (h, *m, mesh_location_rx)),
            greengrass,
            dds_handle
                .as_ref()
                .map(|h| (h, identity.device_id.as_str())),
        );
        let cycle_pulse = watchdog.register(format!("bus:{}", bus.name));
        let direction = bus.serial.direction_mode(cfg.direction)?;
        // This bus's own `CaptureHandle` (same shared `tx`/`dropped` as every other bus, its own
        // `bus` label) — `None` when `[capture]` isn't configured.
        let capture = capture_handle_base
            .as_ref()
            .map(|(tx, dropped)| CaptureHandle {
                tx: tx.clone(),
                dropped: Arc::clone(dropped),
                bus: bus.name.clone(),
            });

        let mut platform = HostPlatform::build_port(
            publisher,
            taps,
            bus.name.clone(),
            &bus.serial.port,
            bus.serial.serial_kind(),
            bus.serial.serial_params(),
            direction,
            bus.serial.read_timeout_ms,
            cycle_pulse,
            preflight_inputs(&cfg, bus),
            capture,
            profile.rate_limit_period,
        )?;

        // Run preflight before entering this bus's poll loop; refuse to start on any failed check
        // EXCEPT `serial_present` — a missing/unopenable serial port is exactly the condition
        // `build_port` above already degrades gracefully for (broker up, `HostSerial::Disconnected`,
        // `reconnect()` retries in the background — see bug #172). Failing hard here would
        // re-introduce the crash-loop that fix removed; every other check (config validity, cloud
        // creds, mosquitto bridge) still blocks startup as before.
        let report = platform.preflight().await;
        if !report.ok_except("serial_present") {
            anyhow::bail!(
                "bus {}: preflight failed ({} failed check(s)); refusing to start poll loop",
                bus.name,
                report.failed()
            );
        }

        let rc = run_config(&cfg, bus, identity);
        // Dispatch the poll loop by protocol (D9): Seplos → `impl_app::run` (battery), Voltronic →
        // `impl_app::run_inverter` (inverter). Both return `!` (never return) — the async block
        // coerces that to `()` so it fits `JoinSet<()>` uniformly with every other spawned task.
        match identity.protocol {
            ProtocolCfg::Seplos => {
                port_tasks.spawn(async move {
                    run(platform, rc).await;
                });
            }
            ProtocolCfg::Voltronic => {
                port_tasks.spawn(async move {
                    run_inverter(platform, rc).await;
                });
            }
        }
    }

    // Tell systemd we're up (Type=notify) once every bus's platform is built and its poll loop is
    // launched. No-op if not run under such a unit.
    notify_ready();

    let watchdog_task = tokio::spawn(watchdog.run(WATCHDOG_WINDOW_MS));

    // ---- Shutdown: SIGTERM / Ctrl-C / an unexpected task exit, whichever comes first -------------
    tokio::select! {
        _ = wait_for_sigterm() => log::info!("SIGTERM received; shutting down"),
        _ = tokio::signal::ctrl_c() => log::info!("shutdown signal received; stopping"),
        joined = port_tasks.join_next(), if !port_tasks.is_empty() => {
            log::warn!("a bus poll-loop task exited unexpectedly ({joined:?}); shutting down");
        }
    }

    notify_stopping();
    watchdog_task.abort();
    port_tasks.abort_all();
    let _ = shutdown_tx.send(true);

    // Await every worker's exit — the (already-aborted) port-loop tasks and the secondary
    // background tasks (which see the shutdown broadcast above) — bounded by ONE 5 s drain
    // timeout, never hanging the shutdown regardless of how many tasks are outstanding.
    let _ = tokio::time::timeout(DRAIN_TIMEOUT, async {
        while port_tasks.join_next().await.is_some() {}
        while secondary.join_next().await.is_some() {}
    })
    .await;

    Ok(())
}

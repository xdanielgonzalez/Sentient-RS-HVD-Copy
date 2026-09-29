//! `lib-locate` — multi-tier location service.
//!
//! Tags outbound telemetry with a best-effort [`LocationFix`], falling back through an ordered
//! [`LocationChain`] of [`LocationProvider`]s: `Manual` (a fixed config-supplied fix) > `GpsNmea`
//! (serial GPS/GNSS receiver) > `ModemManager` (cellular-GPS via the `mmcli` CLI) > `IpGeo`
//! (IP-geolocation over plain HTTP) — the legacy AWS pipeline's location service
//! (`example/aws/.../location`), not yet re-homed into sentient-rs (see
//! `docs/gap-analysis-meteor-parity.md`'s "Multi-tier location service" finding). That priority
//! order ("manual override wins") is [`LocationChain::order`]'s *default* value, not a hardcoded
//! walk — see `impl_chain.rs`.
//!
//! HTTPS is deliberately out of scope for the `IpGeo` tier (see `impl_ip_geo.rs`'s doc comment):
//! adding a second, independently-resolved `rustls` version would trip the workspace's
//! duplicate-TLS-crate ban for a best-effort, secret-free location tag.
//!
//! std-only (the RPi deployment target — see CLAUDE.md "Platform & Deployment"); never added to a
//! `no_std` dependency line.

mod prelude;
mod types;

mod impl_chain;
mod impl_gps_nmea;
mod impl_ip_geo;
mod impl_manual;
mod impl_modem_manager;
mod impl_provider_kind;

pub use types::*;

/// A fix survives this many missed refresh ticks before [`run_refresher`] expires it back to
/// `None` (see `impl_chain.rs`'s `resolve_tick`) — generous enough that one or two transient tier
/// failures (a GPS cold-fix retry, a busy modem) don't flap the published location, but bounded so
/// a permanently lost fix doesn't linger forever.
///
/// This is an engineering choice for this crate, not copied from the legacy baseline (whose exact
/// staleness policy isn't available in this sandbox — see `docs/gap-analysis-meteor-parity.md`'s
/// "Multi-tier location service" finding) — not a PROVISIONAL/reference-derived value.
const STALE_TICKS: u64 = 3;

/// Wall-clock milliseconds since the Unix epoch, saturating to `0` on a clock error (never panics
/// — this crate denies `unwrap`/`expect`/`panic` in production code) rather than a real fix.
pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Periodically refresh the location fix through `chain`'s fallback order (first successful tier
/// wins each tick — see `impl_chain::attempt_once`), publishing the result onto `tx` every
/// `refresh_secs`, until `shutdown` reports true. A fix that stops refreshing (every tier failing)
/// is kept published for a grace window before expiring to `None` — see `STALE_TICKS` /
/// `impl_chain::resolve_tick`.
///
/// `chain`'s own `refresh_secs` field is intentionally not read here — `refresh_secs` (the
/// parameter) is the single source of truth for this call's cadence, matching how
/// `bin-rpi/src/runtime.rs` already calls this function (`loc_cfg.refresh_secs` on both).
pub async fn run_refresher(
    chain: LocationChain,
    refresh_secs: u64,
    tx: tokio::sync::watch::Sender<Option<LocationFix>>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    log::info!("lib-locate: run_refresher started");
    let mut providers = chain.build_providers();
    if providers.is_empty() {
        log::warn!(
            "lib-locate: no location tier is configured/enabled; every tick will publish None"
        );
    }
    let refresh_secs = refresh_secs.max(1);
    let stale_after_ms = STALE_TICKS
        .saturating_mul(refresh_secs)
        .saturating_mul(1_000);
    let mut last: Option<LocationFix> = None;
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(refresh_secs));
    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    break;
                }
            }
            _ = interval.tick() => {
                let fresh = impl_chain::attempt_once(&mut providers).await;
                let resolved = impl_chain::resolve_tick(fresh, &mut last, now_ms(), stale_after_ms);
                let _ = tx.send(resolved);
            }
        }
    }
    log::info!("lib-locate: run_refresher stopped");
}

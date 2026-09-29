//! [`UiSnapshot`] — the single, `Copy`-able data shape all four OLED screens read at render time.
//!
//! Fields are small `Copy` primitives, never a richer canonical-library type, so `ui` stays
//! decoupled from `lib-canonical`/`lib-net`/`lib-mapping`. A later wiring wave maps those models
//! into this shape on each publish to an `embassy_sync::watch::Watch<_, UiSnapshot, _>` —
//! always-latest, non-blocking send, so producer tasks never stall on the UI. Numeric fields keep
//! `CanonicalReading`'s raw fixed-point scale (deci-percent, centi-volt, centi-amp); screens divide
//! to human units at draw time. Exception: the two INA3221 rail-draw fields are pre-mapped to
//! milliamps (in `bin_mcu_platform::publish_rail_draw`) — they are the adapter board's own
//! diagnostic draw, not battery/pack data.

use super::dhcp_state::DhcpState;

/// The frozen data shape every [`super::screen::ScreenView::render`] call reads. `Copy` + plain
/// scalars only — no alloc, no borrows — so a wiring task can snapshot it out of a `Watch` with a
/// cheap copy rather than holding a lock across the render call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiSnapshot {
    // ---- Network screen ----
    /// Current DHCP/link state.
    pub dhcp_state: DhcpState,
    /// Bound IPv4 address, big-endian octets. Meaningless (ignored by the renderer) unless
    /// `dhcp_state == DhcpState::Bound`.
    pub ip_addr: Option<[u8; 4]>,
    /// Whether the device-hosted MQTT broker is currently accepting connections.
    pub broker_up: bool,

    // ---- Meshtastic screen ----
    /// Meshtastic node number of THIS device (0 = not yet learned from the radio).
    pub mesh_node_num: u32,
    /// Running count of mesh frames sent since boot.
    pub mesh_frames_sent: u32,
    /// Whether the most recent mesh send completed without a transport error.
    pub mesh_last_send_ok: bool,

    // ---- Battery screen (Seplos BMS — the project's actual power source) ----
    /// State of charge, deci-percent (×0.1 %) — same scale as `CanonicalReading::soc_dpct`.
    pub battery_soc_dpct: u16,
    /// Pack voltage, centi-volts (×0.01 V) — same scale as `CanonicalReading::total_voltage_cv`.
    pub pack_voltage_cv: u16,
    /// Pack current, signed centi-amps (×0.01 A) — same scale/sign convention as
    /// `CanonicalReading::current_ca` (positive = charging, vendor convention).
    pub pack_current_ca: i16,

    // ---- About / device-diagnostics screen ----
    /// Firmware version string (e.g. `env!("CARGO_PKG_VERSION")` at the wiring call site).
    /// `&'static str` keeps `UiSnapshot` `Copy` — no owned/heap string.
    pub fw_version: &'static str,
    /// Seconds since boot.
    pub uptime_secs: u32,
    /// W5500 Ethernet rail DRAW, milliamps — the adapter board's OWN rail current (INA3221
    /// diagnostics), already mapped from the raw shunt/bus pair (see this file's module doc). NOT
    /// battery/pack data — deliberately rendered on the About/device screen, not Battery, so it is
    /// never mistaken for Seplos-sourced mission telemetry. Signed: the INA3221 shunt-voltage
    /// register is a signed two's-complement field even though this rail is expected
    /// one-directional.
    pub rail_eth_ma: i16,
    /// Heltec V4 LoRa radio rail DRAW, milliamps — same provenance/caveat as `rail_eth_ma`.
    pub rail_radio_ma: i16,
}

use serde::Deserialize;

/// DDS/RTPS (MIL-STD-3071) transport config — presence of `[dds]` enables the DDS stack
/// alongside MQTT (MQTT stays always-on regardless); ABSENCE means DDS is off entirely (the
/// pre-#324 behavior, unchanged). Mirrors `BrokerToml`/`CloudConfig`'s `deny_unknown_fields` +
/// defaulted-field shape; overridable via the standard `SENTIENT_DDS__*` env pattern
/// (`Config::load`'s `Env::prefixed("SENTIENT_").split("__")` applies uniformly to every section,
/// this one included — e.g. `SENTIENT_DDS__DOMAIN_ID=0`).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DdsToml {
    /// DDS domain (MIL-STD-3071 mandates domain 0 — the default).
    #[serde(default)]
    pub domain_id: u16,
    /// Network interface carrying the SPDP multicast (e.g. `"eth0"`) — see
    /// `lib_dds::DdsConfig::iface`'s doc comment. Optional; `runtime::supervise` falls back to
    /// `"eth0"` when omitted.
    #[serde(default)]
    pub iface: Option<String>,
    /// The address advertised in this device's DDS unicast locators, as an IPv6 literal (e.g.
    /// `"fe80::215:5dff:fee2:3c1d"`). Optional — omitted means `lib_dds::DdsConfig`'s own default
    /// (`::1` loopback, same-host only). See `lib_dds::DdsConfig::unicast_addr`'s doc comment:
    /// **required** (set to `iface`'s own routable/link-local address) for cross-host or external
    /// CycloneDDS interop — a peer participant on another host (or reached only via `iface`, not
    /// loopback) cannot route back to a writer that advertises `::1`.
    #[serde(default)]
    pub unicast_addr: Option<String>,
}

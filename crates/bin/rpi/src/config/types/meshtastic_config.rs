use serde::Deserialize;

/// Smallest `max_bytes` that can ever produce a non-empty condensed frame: `lib_net::mesh::condense`
/// returns `0` (nothing to send) whenever its capacity is below its own `EMPTY_OBJECT_LEN` (the
/// `{}` empty-object floor) — see [`MeshtasticConfig::validate`].
pub const MIN_MESHTASTIC_MAX_BYTES: usize = 2;

/// Meshtastic mesh-radio secondary publish target — see
/// `docs/gap-analysis-meteor-parity.md`'s "Meshtastic mesh-radio forwarding" finding. Mirrors
/// `lib_net::mesh::MeshConfig`'s fields (via [`super::MeshtasticConfig::to_mesh_config`], in
/// `impl_meshtastic_config.rs`), with the wire/config-facing field names this workstream's brief
/// specifies (`port`/`max_bytes`/`min_send_interval_ms`) rather than `lib_net::mesh::MeshConfig`'s
/// internal Rust field names (`device_path`/`max_payload`/`send_interval_ms` — unchanged there
/// since `bin-rpi/src/runtime.rs` already reads `MeshConfig::max_payload` directly).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeshtasticConfig {
    /// Explicit enable/disable switch WITHIN an already-present `[meshtastic]` section — defaults
    /// to `true`, preserving this crate's established "presence of the section enables the tap"
    /// convention (see `disk_config.rs`'s doc comment: "mirrors the `[meshtastic]`/`[greengrass]`/
    /// `[location]` opt-in-by-presence pattern") while still letting a checked-in config disable
    /// the tap without deleting the whole section. NOT YET CONSULTED by `runtime::supervise`
    /// (owned by a different work stream in this umbrella story) — presence of `[meshtastic]` is
    /// still the only thing that currently gates spawning the radio task; wiring this flag into
    /// that gate is a follow-up.
    #[serde(default = "default_meshtastic_enabled")]
    pub enabled: bool,
    /// Serial device path the Meshtastic radio is attached on.
    #[serde(default = "default_meshtastic_port")]
    pub port: String,
    /// Serial baud rate. Must be non-zero — see [`MeshtasticConfig::validate`].
    #[serde(default = "default_meshtastic_baud")]
    pub baud: u32,
    /// Meshtastic channel index to send on (`MeshPacket.channel`; `0` = the radio's primary
    /// channel). Left unbounded: this crate has no local range check for the on-wire Meshtastic
    /// channel-index encoding to verify a bound against (see CLAUDE.md's
    /// don't-invent-external-tool-facts policy) — an out-of-range value is the radio firmware's
    /// concern, not this config layer's.
    #[serde(default)]
    pub channel: u32,
    /// Maximum on-air payload size in bytes (the baseline's greedy condensation budget, ~200).
    /// Must be `>=` [`MIN_MESHTASTIC_MAX_BYTES`] — see [`MeshtasticConfig::validate`].
    #[serde(default = "default_meshtastic_max_bytes")]
    pub max_bytes: usize,
    /// Minimum interval between mesh sends, in milliseconds (LoRa duty-cycle friendliness). Must
    /// be non-zero — see [`MeshtasticConfig::validate`].
    #[serde(default = "default_meshtastic_min_send_interval_ms")]
    pub min_send_interval_ms: u64,
}

fn default_meshtastic_enabled() -> bool {
    true
}
fn default_meshtastic_port() -> String {
    "/dev/ttyUSB1".into()
}
fn default_meshtastic_baud() -> u32 {
    115_200
}
fn default_meshtastic_max_bytes() -> usize {
    200
}
fn default_meshtastic_min_send_interval_ms() -> u64 {
    30_000
}

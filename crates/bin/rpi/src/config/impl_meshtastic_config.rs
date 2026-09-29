use super::{MeshtasticConfig, MIN_MESHTASTIC_MAX_BYTES};
use lib_net::mesh::MeshConfig;

impl MeshtasticConfig {
    /// Map to the `lib-mesh` radio/condensation config `lib_net::mesh::run_radio` takes. `enabled` is
    /// intentionally NOT carried across — see this type's doc comment for why it isn't consulted
    /// yet.
    pub fn to_mesh_config(&self) -> MeshConfig {
        MeshConfig {
            device_path: self.port.clone(),
            baud: self.baud,
            channel: self.channel,
            max_payload: self.max_bytes,
            send_interval_ms: self.min_send_interval_ms,
        }
    }

    /// Reject a `baud` of `0`, a `max_bytes` too small to ever hold a condensed frame, or a
    /// `min_send_interval_ms` of `0`.
    ///
    /// - `baud = 0` is not a serial line rate any hardware speaks.
    /// - `max_bytes < `[`MIN_MESHTASTIC_MAX_BYTES`]: `lib_net::mesh::condense`'s own cap check
    ///   (`cap < EMPTY_OBJECT_LEN`) makes it return `0` bytes written whenever the budget can't even
    ///   hold `"{}"`; `bin_rpi_platform::impl_host_tap::send_mesh_frame` then takes the `n == 0`
    ///   branch and never enqueues a frame at all — the radio silently sends nothing, forever, with
    ///   no error anywhere.
    /// - `min_send_interval_ms = 0`: `lib_net::mesh::run_radio` builds `min_interval =
    ///   Duration::from_millis(0)`, so the `elapsed < min_interval` rate-limit check is never true —
    ///   this field's entire documented purpose ("LoRa duty-cycle friendliness") is silently
    ///   disabled rather than merely relaxed. The actual duty-cycle percentage is
    ///   region/hardware-specific, so only the degenerate "no limiting at all" value is rejected;
    ///   any positive value is left to the operator's judgement.
    ///
    /// Fails loudly rather than clamping — matching this config layer's established posture (see
    /// `PollConfig::validate`/`DdsToml::validate`).
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.baud == 0 {
            anyhow::bail!("config error: [meshtastic].baud = 0 is not a usable serial line rate");
        }
        if self.max_bytes < MIN_MESHTASTIC_MAX_BYTES {
            anyhow::bail!(
                "config error: [meshtastic].max_bytes = {} is too small to ever hold a condensed \
                 frame (lib_net::mesh::condense returns 0 bytes below {MIN_MESHTASTIC_MAX_BYTES}); \
                 the radio would silently send nothing forever",
                self.max_bytes
            );
        }
        if self.min_send_interval_ms == 0 {
            anyhow::bail!(
                "config error: [meshtastic].min_send_interval_ms = 0 disables LoRa duty-cycle \
                 rate-limiting entirely (Duration::from_millis(0) never rate-limits) — set a real \
                 minimum interval"
            );
        }
        Ok(())
    }
}

#[cfg(test)]
// Test code may unwrap/panic; the workspace lint gate denies these in production logic only.
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn cfg(baud: u32, max_bytes: usize, min_send_interval_ms: u64) -> MeshtasticConfig {
        MeshtasticConfig {
            enabled: true,
            port: "/dev/ttyUSB1".to_string(),
            baud,
            channel: 0,
            max_bytes,
            min_send_interval_ms,
        }
    }

    #[test]
    fn validate_rejects_zero_baud() {
        assert!(cfg(0, 200, 30_000).validate().is_err());
    }

    #[test]
    fn validate_rejects_max_bytes_below_the_empty_object_floor() {
        assert!(cfg(115_200, 1, 30_000).validate().is_err());
    }

    #[test]
    fn validate_accepts_max_bytes_at_the_empty_object_floor() {
        assert!(cfg(115_200, MIN_MESHTASTIC_MAX_BYTES, 30_000)
            .validate()
            .is_ok());
    }

    #[test]
    fn validate_rejects_zero_min_send_interval_ms() {
        assert!(cfg(115_200, 200, 0).validate().is_err());
    }

    #[test]
    fn validate_accepts_the_documented_default() {
        assert!(cfg(115_200, 200, 30_000).validate().is_ok());
    }
}

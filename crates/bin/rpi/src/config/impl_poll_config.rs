use super::prelude::*;

impl PollConfig {
    /// Reject an `interval_ms` that would publish `DcMeasurementUpdate` slower than its
    /// MIL-STD-3071 nominal cadence.
    ///
    /// `DcMeasurementUpdate` is spec Medium / Periodic 1 s with a 3 s deadline (IDL 353-361;
    /// MIL-STD-3071 §4.9.10.4) and — per `bin_rpi_platform::EmitCadence`'s doc comment — is
    /// deliberately **NOT rate-gated** in the emit path (only the two 10 s topics,
    /// `PowerHardwareUpdate`/`StorageUpdate`, are held to a rate limit there). That means the raw
    /// poll loop's `interval_ms` directly sets `DcMeasurementUpdate`'s publish rate: any value
    /// above [`MAX_INTERVAL_MS_FOR_1HZ`] silently under-publishes it below the mandated 1 Hz (a
    /// real regression that shipped in every deployed config at `interval_ms = 2000` before this
    /// check existed). Fails loudly rather than clamping — matching this config layer's existing
    /// "fail loudly, don't silently correct" posture (see `Config::buses`/`Config::identities`) —
    /// so a misconfigured device refuses to boot instead of quietly shipping out-of-spec
    /// telemetry.
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.interval_ms == 0 || self.interval_ms > MAX_INTERVAL_MS_FOR_1HZ {
            anyhow::bail!(
                "config error: [poll].interval_ms = {} violates the MIL-STD-3071 1 Hz \
                 DcMeasurementUpdate requirement (Periodic 1 s, IDL 353-361, §4.9.10.4); \
                 DcMeasurementUpdate is NOT rate-gated on the emit path (see \
                 bin_rpi_platform::EmitCadence), so the poll interval directly sets its publish \
                 rate — set interval_ms to 1..={MAX_INTERVAL_MS_FOR_1HZ}",
                self.interval_ms
            );
        }
        // An empty `dip_addresses` is legal to parse but polls nothing every cycle — no operator
        // configures a bus to watch zero addresses on purpose; it is either a stripped-out default
        // or a typo'd `[[bus]]`/`[serial]` split. Fail loudly rather than silently idling the bus.
        if self.dip_addresses.is_empty() {
            anyhow::bail!(
                "config error: [poll].dip_addresses is empty; a bus configured to poll zero \
                 addresses publishes nothing every cycle — set at least one DIP address"
            );
        }
        // Each entry must be encodable as the Seplos protocol's one-hex-digit DIP address
        // (`lib_parse::seplos::command`, `SeplosError::AddrOutOfRange`). A value outside 0..=15
        // parses fine as a `u8` here but fails command-encoding on every poll cycle forever —
        // never caught at startup, only as a per-cycle comms error indistinguishable from a dead
        // bus. Bounded uniformly (even though a Voltronic bus never reads `dip_addresses` at all)
        // since `PollConfig` is shared across both protocols and the default is already in-range.
        if let Some(&bad) = self
            .dip_addresses
            .iter()
            .find(|&&d| d > MAX_SEPLOS_DIP_ADDRESS)
        {
            anyhow::bail!(
                "config error: [poll].dip_addresses contains {bad}, which is outside the \
                 Seplos-encodable DIP address range 0..={MAX_SEPLOS_DIP_ADDRESS} \
                 (SeplosError::AddrOutOfRange) — every poll of that address would fail forever"
            );
        }
        Ok(())
    }
}

impl Default for PollConfig {
    fn default() -> Self {
        PollConfig {
            dip_addresses: vec![0],
            // TMS-required nominal cadence: `DcMeasurementUpdate` is spec Medium / Periodic 1 s with
            // a 3 s deadline (IDL 353-361; MIL-STD-3071 §4.9.10.4). The 1 s default poll interval
            // meets that nominal rate; `PowerHardwareUpdate`/`StorageUpdate` (spec Slow / Periodic
            // 10 s) are held to their own rate by the emit-path rate limiter (see
            // `bin_rpi_platform::EmitCadence`), so a faster poll does not over-publish those.
            interval_ms: 1000,
            fault_every: 10,
        }
    }
}

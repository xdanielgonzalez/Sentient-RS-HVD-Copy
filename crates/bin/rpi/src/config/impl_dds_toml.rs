use super::prelude::*;

impl DdsToml {
    /// Reject a `domain_id` other than 0.
    ///
    /// MIL-STD-3071 §D.2 mandates DDS domain 0 for the RTPS transport (SPDP discovery on the
    /// domain-0 multicast group, `ff02::1234`) — a non-zero `domain_id` would put this
    /// participant in a different discovery domain than every compliant peer, so it silently
    /// stops interoperating instead of failing loudly. `u16::default()` happening to be `0`
    /// is not enforcement: an explicit `domain_id = 7` in the TOML parses fine without this
    /// check. Fails loudly rather than clamping — matching this config layer's existing
    /// "fail loudly, don't silently correct" posture (see `PollConfig::validate`/
    /// `Config::buses`/`Config::identities`).
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.domain_id != 0 {
            anyhow::bail!(
                "config error: [dds].domain_id = {} violates MIL-STD-3071 §D.2, which mandates \
                 DDS domain 0 — set domain_id = 0 (or omit it; it defaults to 0)",
                self.domain_id
            );
        }
        Ok(())
    }
}

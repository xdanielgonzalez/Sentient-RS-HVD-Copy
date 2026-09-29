use super::prelude::*;

impl Default for BrokerToml {
    fn default() -> Self {
        BrokerToml {
            host: "127.0.0.1".into(),
            port: 1883,
        }
    }
}

impl BrokerToml {
    /// Reject `port = 0`.
    ///
    /// `0` means "let the OS pick an ephemeral bind port" — nonsensical here, because
    /// `broker.port` is also the FIXED port every other in-process component (the mosquitto
    /// bridge, the heartbeat/reply-listener `rumqttc` clients, `mc_select` — see
    /// `runtime::supervise`) connects back to on loopback, already resolved from this same config
    /// value before the broker ever binds. An ephemeral port chosen at bind time would leave every
    /// one of those clients pointed at a port nothing is listening on. Fails loudly rather than
    /// clamping — matching this config layer's established "fail loudly, don't silently correct"
    /// posture (see `PollConfig::validate`/`DdsToml::validate`).
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.port == 0 {
            anyhow::bail!(
                "config error: [broker].port = 0 is not a usable fixed port; the embedded \
                 broker's bind port is also what every in-process MQTT client connects back to"
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

    #[test]
    fn validate_rejects_port_zero() {
        let cfg = BrokerToml {
            host: "127.0.0.1".into(),
            port: 0,
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn validate_accepts_the_default() {
        assert!(BrokerToml::default().validate().is_ok());
    }
}

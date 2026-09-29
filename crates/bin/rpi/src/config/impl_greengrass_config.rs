use super::prelude::*;

impl GreengrassConfig {
    /// Reject a protocol-invalid `qos` or a `port` that can never be connected to — same rationale
    /// as [`CloudConfig::validate`] (this bridge mirrors that one's mTLS shape one-for-one; see
    /// `GreengrassConfig`'s doc comment).
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.qos > MAX_MQTT_QOS {
            anyhow::bail!(
                "config error: [greengrass].qos = {} is not a valid MQTT QoS (MQTT 3.1.1 §4.3 \
                 defines only 0, 1, or 2)",
                self.qos
            );
        }
        if self.port == 0 {
            anyhow::bail!(
                "config error: [greengrass].port = 0 can never be connected to; set the \
                 Greengrass core broker's real port"
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

    fn cfg(port: u16, qos: u8) -> GreengrassConfig {
        GreengrassConfig {
            host: "gg-core.local".to_string(),
            port,
            cert_file: "/cert.pem".to_string(),
            key_file: "/key.pem".to_string(),
            ca_file: "/ca.pem".to_string(),
            remote_clientid: None,
            local_topic: "telemetry/#".to_string(),
            remote_topic_prefix: String::new(),
            qos,
            component_name: String::new(),
        }
    }

    #[test]
    fn validate_accepts_every_valid_qos_level() {
        for qos in 0..=MAX_MQTT_QOS {
            assert!(cfg(8883, qos).validate().is_ok(), "qos={qos} must be valid");
        }
    }

    #[test]
    fn validate_rejects_qos_above_2() {
        let err = cfg(8883, 3).validate().expect_err("qos=3 must be rejected");
        assert!(err.to_string().contains("QoS"), "{err}");
    }

    #[test]
    fn validate_rejects_port_zero() {
        assert!(cfg(0, 1).validate().is_err());
    }

    #[test]
    fn validate_accepts_a_real_port() {
        assert!(cfg(8883, 1).validate().is_ok());
    }
}

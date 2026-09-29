use super::prelude::*;

/// Manual `Debug`: enumerates every field explicitly (rather than deriving) so that if a future
/// edit ever adds a secret field to this struct, it does not silently start appearing in logs —
/// whoever adds the field has to also decide how (or whether) to redact it here. Today there is no
/// secret field on this struct (the password lives only in the `CLOUD_MQTT_PASS` env var), so this
/// prints everything.
impl std::fmt::Debug for CloudConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CloudConfig")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("username", &self.username)
            .field("ca_file", &self.ca_file)
            .field("remote_clientid", &self.remote_clientid)
            .field("topic", &self.topic)
            .field("qos", &self.qos)
            .finish()
    }
}

impl CloudConfig {
    /// The `CLOUD_MQTT_*` environment names this config maps to (documents the naming contract
    /// shared with `distro/deploy/mosquitto.conf`'s historical placeholders / `distro/deploy/cloud.env.example`).
    /// Deliberately excludes the password — that name (`CLOUD_MQTT_PASS`) is env-only and never
    /// derived from this struct.
    pub fn env_map(&self) -> Vec<(&'static str, String)> {
        let mut m = vec![
            ("CLOUD_MQTT_HOST", self.host.clone()),
            ("CLOUD_MQTT_PORT", self.port.to_string()),
            ("CLOUD_MQTT_USER", self.username.clone()),
            ("CLOUD_MQTT_CAFILE", self.ca_file.clone()),
        ];
        if let Some(id) = &self.remote_clientid {
            m.push(("CLOUD_MQTT_CLIENTID", id.clone()));
        }
        m
    }

    /// Reject a protocol-invalid `qos` or a `port` that can never be connected to.
    ///
    /// `qos` — MQTT 3.1.1 §4.3 defines exactly three QoS levels (0/1/2); anything else parses fine
    /// as a `u8` but is meaningless to every MQTT client/broker on the wire — currently caught only
    /// downstream (if at all) by mosquitto's own parser on the rendered bridge conf. `port` — `0`
    /// is never a real *destination* port (unlike a bind address, where `0` means "OS picks one");
    /// a bridge configured to connect to port 0 fails to connect, forever, with no bridge traffic
    /// ever flowing and no config-time signal that anything is wrong. Fails loudly rather than
    /// clamping — matching this config layer's established "fail loudly, don't silently correct"
    /// posture (see `PollConfig::validate`/`DdsToml::validate`).
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.qos > MAX_MQTT_QOS {
            anyhow::bail!(
                "config error: [cloud].qos = {} is not a valid MQTT QoS (MQTT 3.1.1 §4.3 defines \
                 only 0, 1, or 2)",
                self.qos
            );
        }
        if self.port == 0 {
            anyhow::bail!(
                "config error: [cloud].port = 0 can never be connected to; set the private-cloud \
                 broker's real port"
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

    fn cfg(port: u16, qos: u8) -> CloudConfig {
        CloudConfig {
            host: "mqtt.sentient.example".to_string(),
            port,
            username: "u".to_string(),
            ca_file: "/ca.pem".to_string(),
            remote_clientid: None,
            topic: "tms/#".to_string(),
            qos,
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

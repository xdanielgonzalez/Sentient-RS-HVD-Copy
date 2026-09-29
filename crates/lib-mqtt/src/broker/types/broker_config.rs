use super::connect_auth::ConnectAuth;

/// Configuration for the embedded broker.
#[derive(Debug, Clone)]
pub struct BrokerConfig {
    /// Broker/router id.
    pub id: String,
    /// Listener bind address. **Loopback by default** — the mosquitto relay bridge subscribes
    /// locally, so the embedded broker must not be reachable off-host unless a deployment
    /// deliberately widens this via config.
    pub host: String,
    /// TCP port (default 1883).
    pub port: u16,
    /// CONNECT authorizer, routed through the `lib-security` seam via [`crate::broker::seam_auth`]. `None`
    /// (the default) leaves the broker unauthenticated — correct for the loopback-only default
    /// posture (mosquitto relay + local subscribers), but a deployment that widens `host` beyond
    /// loopback MUST set this.
    pub auth: Option<ConnectAuth>,
}

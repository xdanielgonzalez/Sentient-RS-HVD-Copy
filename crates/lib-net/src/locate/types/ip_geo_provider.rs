/// The IP-geolocation tier: a minimal hand-rolled HTTP/1.1 GET over a `tokio::net::TcpStream`
/// against a configurable `http://` endpoint — see `impl_ip_geo.rs`'s doc comment for why HTTPS is
/// deferred (a second `rustls` version would trip the workspace's duplicate-TLS-crate ban).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct IpGeoProvider {
    /// `http://host[:port]/path`.
    pub url: String,
    pub timeout_ms: u64,
}

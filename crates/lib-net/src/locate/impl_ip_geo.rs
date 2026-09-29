use super::prelude::*;
use std::time::Duration;
use tokio::net::TcpStream;

impl LocationProvider for IpGeoProvider {
    /// Minimal HTTP/1.1 GET against `self.url`. HTTPS is deferred: the workspace already pins one
    /// `rustls` version for `lib-link`'s TLS record layer (see that dependency's Cargo.toml
    /// comment), and cargo-deny bans duplicate crate *versions* — adding a second, independently
    /// resolved TLS stack here risks exactly that, for a "best-effort location tag" feature that
    /// doesn't carry secrets. Configure `ip_geo_url` behind a trusted local proxy/VPN if the
    /// endpoint itself needs to be reached privately.
    async fn locate(&mut self) -> Result<LocationFix, &'static str> {
        let (host, port, path) = parse_http_url(&self.url)
            .ok_or("ip-geo: invalid url (expected http://host[:port]/path)")?;
        let budget = Duration::from_millis(self.timeout_ms.max(1));
        let body = match tokio::time::timeout(budget, fetch(&host, port, &path)).await {
            Ok(result) => result?,
            Err(_) => return Err("ip-geo: request timed out"),
        };
        let (lat, lon) = parse_ip_geo_body(&body).ok_or("ip-geo: response missing lat/lon")?;
        Ok(LocationFix {
            lat,
            lon,
            alt_m: 0.0,
            source: LocationSource::IpGeo,
            ts_ms: crate::locate::now_ms(),
        })
    }
}

/// Open a TCP connection to `host:port`, send a `Connection: close` GET for `path`, and return the
/// response body's raw bytes.
async fn fetch(host: &str, port: u16, path: &str) -> Result<Vec<u8>, &'static str> {
    let mut stream = TcpStream::connect((host, port))
        .await
        .map_err(|_| "ip-geo: connect failed")?;
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: sentient-rs-lib-locate/1\r\n\
         Accept: application/json\r\nConnection: close\r\n\r\n"
    );
    stream
        .write_all(request.as_bytes())
        .await
        .map_err(|_| "ip-geo: write failed")?;
    let mut raw = Vec::new();
    stream
        .read_to_end(&mut raw)
        .await
        .map_err(|_| "ip-geo: read failed")?;
    split_http_body(&raw)
        .map(<[u8]>::to_vec)
        .ok_or("ip-geo: malformed HTTP response")
}

/// `http://host[:port]/path` -> `(host, port, path)`. Deliberately minimal (no query-string
/// canonicalization, no userinfo) — see the crate doc comment on why HTTPS/a full URL crate is
/// out of scope.
fn parse_http_url(url: &str) -> Option<(String, u16, String)> {
    let rest = url.strip_prefix("http://")?;
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) => (h, p.parse().ok()?),
        None => (authority, 80u16),
    };
    if host.is_empty() {
        return None;
    }
    Some((host.to_string(), port, path.to_string()))
}

/// Split a raw HTTP/1.1 response into its status line + body, honoring `Content-Length` when
/// present (else the whole post-header remainder, which is correct for the `Connection: close`
/// requests this module always sends). `None` on a non-`200` status or malformed framing.
fn split_http_body(raw: &[u8]) -> Option<&[u8]> {
    let header_end = find_subslice(raw, b"\r\n\r\n")?;
    let header = std::str::from_utf8(&raw[..header_end]).ok()?;
    let mut lines = header.lines();
    let status_line = lines.next()?;
    if !status_line.contains(" 200 ") {
        return None;
    }
    let content_length = lines.find_map(|l| {
        l.to_ascii_lowercase()
            .strip_prefix("content-length:")
            .and_then(|v| v.trim().parse::<usize>().ok())
    });
    let body = &raw[header_end + 4..];
    Some(match content_length {
        Some(len) if len <= body.len() => &body[..len],
        _ => body,
    })
}

fn find_subslice(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Extract `lat`/`lon` from the response JSON — matches `ip-api.com/json`'s field names.
///
/// PROVISIONAL (gap-closure #237, issue #241): the default `ip_geo_url` (a real, currently
/// HTTP-reachable third-party service) and this field-name assumption are a best-effort stand-in,
/// not verified against the legacy baseline's actual endpoint/response shape — no reference
/// prototype was available in this sandbox (see `docs/gap-analysis-meteor-parity.md`'s
/// "Multi-tier location service" finding). A deployment pointing `ip_geo_url` at a different
/// provider only needs this function's field names updated to match.
fn parse_ip_geo_body(body: &[u8]) -> Option<(f64, f64)> {
    let value: serde_json::Value = serde_json::from_slice(body).ok()?;
    let lat = value.get("lat")?.as_f64()?;
    let lon = value.get("lon")?.as_f64()?;
    Some((lat, lon))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    #[test]
    fn parses_host_port_and_path() {
        assert_eq!(
            parse_http_url("http://ip-api.com/json").unwrap(),
            ("ip-api.com".to_string(), 80, "/json".to_string())
        );
        assert_eq!(
            parse_http_url("http://127.0.0.1:8080/loc?x=1").unwrap(),
            ("127.0.0.1".to_string(), 8080, "/loc?x=1".to_string())
        );
        assert!(parse_http_url("https://example.com/").is_none());
        assert!(parse_http_url("http://").is_none());
    }

    #[test]
    fn defaults_to_root_path_when_omitted() {
        assert_eq!(
            parse_http_url("http://host:81").unwrap(),
            ("host".to_string(), 81, "/".to_string())
        );
    }

    #[test]
    fn body_json_yields_lat_lon() {
        let body = br#"{"status":"success","lat":40.7128,"lon":-74.006}"#;
        let (lat, lon) = parse_ip_geo_body(body).expect("has lat/lon");
        assert!((lat - 40.7128).abs() < 1e-9);
        assert!((lon - (-74.006)).abs() < 1e-9);
    }

    #[test]
    fn body_missing_lat_lon_is_none() {
        assert!(parse_ip_geo_body(br#"{"status":"fail"}"#).is_none());
    }

    /// Spins a real loopback HTTP server for one request, feeding `IpGeoProvider::locate` a canned
    /// 200 response — proves the socket/header/body-split path end to end, not just the pure
    /// parsing helpers above.
    #[tokio::test]
    async fn locate_against_a_canned_loopback_responder() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("local_addr");

        let server = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.expect("accept");
            let mut buf = [0u8; 1024];
            let _ = sock.read(&mut buf).await.expect("read request");
            let json = br#"{"status":"success","lat":51.5074,"lon":-0.1278}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
                json.len()
            );
            sock.write_all(response.as_bytes())
                .await
                .expect("write header");
            sock.write_all(json).await.expect("write body");
            sock.shutdown().await.expect("shutdown");
        });

        let mut provider = IpGeoProvider {
            url: format!("http://{addr}/json"),
            timeout_ms: 2_000,
        };
        let fix = provider
            .locate()
            .await
            .expect("loopback responder succeeds");
        assert!((fix.lat - 51.5074).abs() < 1e-6);
        assert!((fix.lon - (-0.1278)).abs() < 1e-6);
        assert_eq!(fix.source, LocationSource::IpGeo);

        server.await.expect("server task");
    }

    #[tokio::test]
    async fn locate_times_out_against_an_unresponsive_port() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("local_addr");
        // Never accept — the connect will succeed (backlog) but nothing is ever written back.
        let mut provider = IpGeoProvider {
            url: format!("http://{addr}/json"),
            timeout_ms: 100,
        };
        assert!(provider.locate().await.is_err());
        drop(listener);
    }
}

/// CONNECT packet parameters (a subset of MQTT 3.1.1's CONNECT payload/flags — no Will fields yet).
#[derive(Debug, Clone, Copy)]
pub struct ConnectOptions<'a> {
    /// Client Identifier (UTF-8). The wire format bounds it to 16 bits; broker interop
    /// conventionally wants <= 23 bytes, but the encoder only enforces the wire limit.
    pub client_id: &'a str,
    /// Keep Alive interval, seconds (0 disables the keep-alive timeout).
    pub keep_alive_secs: u16,
    /// Clean Session flag.
    pub clean_session: bool,
    /// Username, if the broker requires auth.
    pub username: Option<&'a str>,
    /// Password, if the broker requires auth (only meaningful alongside `username` per spec).
    pub password: Option<&'a [u8]>,
}

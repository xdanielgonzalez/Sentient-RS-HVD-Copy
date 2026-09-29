use super::prelude::*;

impl TlsStage {
    /// A client-role stage that will verify the peer's certificate against `server_name` (SNI +
    /// identity check) against exactly `trusted_root` — the caller's one trust anchor (see the
    /// module docs for where that material comes from in this workspace's tests).
    pub fn new_client(server_name: &str, trusted_root: CertificateDer<'static>) -> Self {
        let mut roots = RootCertStore::empty();
        // The caller is the same process that generated (or otherwise sourced) `trusted_root`
        // moments earlier; a DER blob that fails to parse here is a caller bug, not a runtime
        // condition — `unreachable!` documents that invariant without threading a `Result`
        // through every call site for a case that cannot happen with well-formed DER.
        roots
            .add(trusted_root)
            .unwrap_or_else(|_| unreachable!("caller-supplied trusted root must be parseable DER"));
        let config = Arc::new(
            ClientConfig::builder()
                .with_root_certificates(roots)
                .with_no_client_auth(),
        );
        // `server_name` is documented (see the module docs) to always be a valid DNS name at
        // every real call site in this workspace; `unreachable!` guards against ever unwrapping
        // a caller-controlled `Result` rather than silently falling back to some other name.
        let name = ServerName::try_from(server_name.to_string())
            .unwrap_or_else(|_| unreachable!("server_name is documented to be a valid DNS name"));
        let conn = ClientConnection::new(config, name).unwrap_or_else(|_| {
            unreachable!("a validly constructed client config is always valid")
        });
        TlsStage {
            role: TlsRole::Client,
            conn: Connection::Client(conn),
        }
    }

    /// A server-role stage presenting `identity`'s certificate chain and signing with its key (see
    /// module docs and [`crate::link::tls_identity`]). Fails closed: an identity that can't produce a
    /// certified key (e.g. a secure-world handle lookup failure) yields [`TlsError::Handshake`]
    /// instead of panicking.
    pub fn new_server(identity: &dyn TlsIdentity) -> Result<Self, TlsError> {
        let resolver = tls_identity::resolver_for(identity)?;
        let config = Arc::new(
            ServerConfig::builder()
                .with_no_client_auth()
                .with_cert_resolver(resolver),
        );
        let conn = ServerConnection::new(config).map_err(|_| TlsError::Handshake)?;
        Ok(TlsStage {
            role: TlsRole::Server,
            conn: Connection::Server(conn),
        })
    }

    /// Which role this stage plays.
    pub fn role(&self) -> TlsRole {
        self.role
    }

    /// Feed raw bytes just read from the underlying TCP socket into the record layer (handshake
    /// flight or an encrypted application-data record).
    pub fn feed_incoming(&mut self, bytes: &[u8]) -> Result<(), TlsError> {
        let mut reader = Cursor::new(bytes);
        self.conn
            .read_tls(&mut reader)
            .map_err(|_| TlsError::Protocol)?;
        self.conn
            .process_new_packets()
            .map_err(|_| TlsError::Handshake)?;
        Ok(())
    }

    /// Drain up to `out.len()` bytes the stage wants written to the underlying TCP socket (a
    /// handshake flight or a sealed record), returning how many bytes were written.
    pub fn drain_outgoing(&mut self, out: &mut [u8]) -> usize {
        let mut writer = Cursor::new(out);
        // A bounded in-memory `Cursor<&mut [u8]>` write never errors (it just stops accepting
        // bytes once full, which `write_tls` treats as "nothing more fits this call").
        let _ = self.conn.write_tls(&mut writer);
        writer.position() as usize
    }

    /// True until the handshake completes; while true, [`Self::write_plaintext`] /
    /// [`Self::read_plaintext`] have nothing to do yet.
    pub fn is_handshaking(&self) -> bool {
        self.conn.is_handshaking()
    }

    /// Queue `data` as plaintext application data to be sealed into an outgoing record (fetched via
    /// [`Self::drain_outgoing`]).
    pub fn write_plaintext(&mut self, data: &[u8]) -> Result<(), TlsError> {
        self.conn
            .writer()
            .write_all(data)
            .map_err(|_| TlsError::BufferTooSmall)
    }

    /// Read already-decrypted application plaintext into `out`, returning the byte count.
    pub fn read_plaintext(&mut self, out: &mut [u8]) -> Result<usize, TlsError> {
        match self.conn.reader().read(out) {
            Ok(n) => Ok(n),
            // No plaintext ready yet on this poll — not a failure, just nothing to report.
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(0),
            Err(_) => Err(TlsError::Protocol),
        }
    }
}

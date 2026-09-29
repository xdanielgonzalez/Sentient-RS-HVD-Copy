use super::prelude::*;

/// Every method below is a **stub that fails closed**. The pattern is uniform on purpose: an
/// operation either returns `Err(TlsError::Unimplemented)`, or — where the signature cannot carry
/// an error — returns the value that keeps the connection *unusable*. There is no method here that
/// a caller can read as "the peer is authenticated" or "the record is trustworthy".
///
/// See [`TlsServerStage`] for the WP-3 work-package description and its measurement gate.
impl<P: SecureProvider> TlsServerStage<P> {
    /// Take ownership of the Secure-world seam for one inbound connection.
    ///
    /// Deliberately infallible: construction makes no security decision, so the refusal belongs on
    /// the operations, where it is observable per method. A constructed stage is inert — it can
    /// never complete a handshake (see [`Self::is_handshaking`]).
    pub fn new(provider: P) -> Self {
        TlsServerStage { provider }
    }

    /// The seam this stage routes every cryptographic operation through.
    pub fn provider(&self) -> &P {
        &self.provider
    }

    /// Feed raw bytes just read from the TCP socket into the record layer — a handshake flight or an
    /// encrypted application-data record. Returns how many bytes were consumed.
    ///
    /// TODO(ADR 0013 WP-3): `UnbufferedServerConnection::process_tls_records`, then dispatch on the
    /// returned state. `InsufficientSizeError` handling belongs here and is one of the two things
    /// ADR 0013 open question 2 flags as unexercised on Cortex-M.
    pub fn feed_incoming(&mut self, _bytes: &[u8]) -> Result<usize> {
        Err(TlsError::Unimplemented)
    }

    /// Drain up to `out.len()` bytes the stage wants written back to the TCP socket — a handshake
    /// flight or a sealed record — returning the byte count written.
    ///
    /// TODO(ADR 0013 WP-3): the `EncodeTlsData` / `TransmitTlsData` arms of the unbuffered state
    /// machine. ADR 0013 open question 9 (does rustls bound *outbound* records to a tunable
    /// fragment size?) is answered here, and its answer sizes the Secure-world record scratch.
    pub fn drain_outgoing(&mut self, _out: &mut [u8]) -> Result<usize> {
        Err(TlsError::Unimplemented)
    }

    /// Queue application plaintext to be sealed into an outgoing record.
    ///
    /// TODO(ADR 0013 WP-3): `WriteTraffic::encrypt`, with the AEAD itself executing in the Secure
    /// world through `RecordCipher` — ADR 0013 §2 keeps traffic keys off the Non-Secure side even
    /// though Non-Secure already holds the plaintext, so that a FIPS boundary would enclose the
    /// records and not merely the handshake.
    pub fn write_plaintext(&mut self, _data: &[u8]) -> Result<usize> {
        Err(TlsError::Unimplemented)
    }

    /// Read already-decrypted application plaintext into `out`, returning the byte count.
    ///
    /// TODO(ADR 0013 WP-3): the `ReadTraffic` arm. Note that a `0` return must mean "no plaintext
    /// ready this poll" and never "stream closed" — the broker pump distinguishes them.
    pub fn read_plaintext(&mut self, _out: &mut [u8]) -> Result<usize> {
        Err(TlsError::Unimplemented)
    }

    /// Whether the handshake is still in progress.
    ///
    /// **Permanently `true` in the skeleton, and that is the fail-closed answer.** This method
    /// cannot return an error, so it returns the value that leaves the connection unusable: a
    /// caller polling for handshake completion never sees it, and no plaintext path opens. A stub
    /// returning `false` would announce a completed handshake — that a peer had been
    /// authenticated — which is exactly the failure mode this crate is written to make impossible.
    pub fn is_handshaking(&self) -> bool {
        true
    }

    /// The trust anchor the peer authenticated against, once the handshake has completed. This is
    /// the identity ADR 0013 §6 / WP-7 keys topic authorization on — an `AnchorId` with no ACL
    /// entry may do nothing, and a slot with no bound identity may do nothing.
    ///
    /// TODO(ADR 0013 WP-3/WP-7): resolved by `PinnedSpkiClientVerifier` during the handshake and
    /// carried into the broker slot by WP-6's accept path. Until then there is no authenticated
    /// peer, so there is no anchor to report and this refuses.
    pub fn peer_anchor(&self) -> Result<AnchorId> {
        Err(TlsError::Unimplemented)
    }
}

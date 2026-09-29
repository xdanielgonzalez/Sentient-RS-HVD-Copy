use super::secure_provider::SecureProvider;

/// The TLS 1.3 server state machine for one inbound connection — **a skeleton** (ADR 0013 WP-3).
///
/// # What it will be
///
/// A wrapper over rustls's `UnbufferedServerConnection`: the I/O-free server state machine, driven
/// by the caller rather than driving a socket itself. The API surface below is deliberately the
/// **same five-method protocol** as the existing host-side `lib_net::link::tls::TlsStage`
/// (`feed_incoming` / `drain_outgoing` / `write_plaintext` / `read_plaintext` / `is_handshaking`),
/// so the two are interchangeable to a caller and the MCU path reuses a shape that already has a
/// working precedent instead of inventing a second one.
///
/// It owns no socket. ADR 0013's zero-blast-radius claim depends on that: `lib-net`'s
/// `async_broker_pump` is already generic over `S: embedded_io_async::{Read, Write}` and obtains
/// sockets from a caller-supplied closure, so inserting TLS is a `TlsPooledSocket` in
/// `crates/bin/mcu-rp2350/ns` plus three lines of `main.rs` — **`lib-net` needs no change to gain TLS**.
///
/// # What it is right now
///
/// Nothing. It holds the provider and refuses every operation with [`TlsError::Unimplemented`]. The
/// two properties that make that safe rather than merely useless:
///
/// * No method returns a success, and in particular
///   [`is_handshaking`](crate::TlsServerStage::is_handshaking) is **permanently `true`** — a stub
///   that reported a completed handshake would tell a caller the peer was authenticated.
/// * Construction is deliberately kept infallible and harmless — it makes no security decision, it
///   only takes ownership of the provider — so that the refusal lives on the *operations*, where a
///   test can observe it method by method. A stage that constructs but can never complete a
///   handshake cannot be mistaken for a working one.
///
/// # TODO(ADR 0013 WP-3) — the measured work package, not a fill-in-the-blanks
///
/// This is flagged as the **risk** work package and it is a **gate**. Two facts make it so:
///
/// * `UnbufferedServerConnection` on Cortex-M is **unexercised territory**. The only official
///   embedded rustls demo (`ctz/rustls-embedded-demo`, embassy on STM32F429ZI) is a **client**. The
///   socket ↔ state-machine glue and `InsufficientSizeError` handling are ours to write, and they
///   belong here — host-tested against a real rustls client — before any target build (open
///   question 2).
/// * **No published flash/RAM figure exists** for rustls + a RustCrypto provider on `thumbv8m`.
///   Every memory number in ADR 0013 §Consequences is derived arithmetic or a client-side figure
///   (open question 3), and `p256` ECDSA-verify performance on a Cortex-M33 is unmeasured (open
///   question 4) — the 28 ms handshake figure is a floor taken from hand-written assembly, not an
///   estimate for pure Rust.
///
/// WP-3's gate is therefore: **a measured `thumbv8m` flash / RAM / heap-high-water figure and a
/// host handshake against a real rustls client, before WP-6 starts.** If the measurement fails,
/// ADR 0013 §3's fallback applies — terminate TLS on the Pi and run the MCU on a private link — and
/// this type is deleted rather than finished. Do not add the rustls dependency ahead of that
/// decision.
///
/// [`TlsError::Unimplemented`]: crate::TlsError::Unimplemented
pub struct TlsServerStage<P: SecureProvider> {
    /// The Secure-world seam every cryptographic operation routes through. Held rather than
    /// borrowed so the stage can be moved into a per-connection task.
    pub(crate) provider: P,
    // TODO(ADR 0013 WP-3): `conn: rustls::unbuffered::UnbufferedServerConnection` goes here,
    // together with the record buffers ADR 0013 §Consequences sizes at ~21 KiB per session
    // (16.6 KiB inbound + ~4 KiB outbound). Those are static/reused buffers, never per-call stack
    // locals — the MCU stack discipline applies to this crate exactly as it does to the rest of the
    // Non-Secure image.
}

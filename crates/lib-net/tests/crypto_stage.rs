//! [`CryptoStage`] host tests — the real seal/open leg of the `[crypto/TLS]` pipeline slot (see
//! `lib_net::channel`'s module doc), driven by `lib-security`'s software `SessionCipher`
//! (`SoftwareSessionStore`, feature `software` there — the default backend, so `--features crypto`
//! alone pulls it in).
//!
//! Two independently-owned [`CryptoStage`] instances stand in for the seal leg and the open leg
//! running in different pipeline positions (per the module's "hardenable in isolation" design);
//! they share one underlying [`SharedCipher`]-wrapped [`SoftwareSessionStore`] (via `Arc<Mutex<_>>`)
//! so a `KeyHandle` established once is valid key material for both — exactly the shape a real
//! deployment has (one key-custody store, two stage processes on either side of it), without this
//! test needing to know how key material is synchronized across processes (out of scope here).
//!
//! No emulator: host-only (feature `crypto`), run under `cargo test -p lib-net --features crypto
//! --test crypto_stage`.

#![cfg(feature = "crypto")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use lib_net::channel::message::CRYPTO_MAX;
use lib_net::channel::{
    drive, Closed, CryptoOp, CryptoStage, CryptoStageError, CryptoUnit, Receiver, Sender, Stage,
    TryRecvError, TrySendError,
};
// `SoftwareSessionStore` follows the same nesting as `SoftwareKeyStore`
// (`lib_security::backends::software::SoftwareKeyStore`) — the concrete backend types live under
// `backends::software`, only the seam traits (`SessionCipher`, `KeyHandle`, ...) are re-exported at
// the crate root. `InsecureTestRng` is the same deterministic test double `SoftwareKeyStore`'s own
// tests seed with (`backends::software::InsecureTestRng`).
use lib_security::backends::software::{InsecureTestRng, SoftwareSessionStore};
use lib_security::{KeyCustody, KeyHandle, SecurityError, SessionCipher};
use std::sync::{Arc, Mutex};

const SEED: u64 = 0xC0FF_EE00_D15E_5EED;

/// Shares one [`SessionCipher`] (here, one [`SoftwareSessionStore`]) across multiple owning
/// [`CryptoStage`] instances — `Arc<Mutex<_>>` under a local newtype so it can implement the
/// foreign `SessionCipher` trait (test-only; the real pipeline wires seal/open as separate
/// processes over the key-custody boundary, not a shared in-process `Arc`).
struct SharedCipher<C>(Arc<Mutex<C>>);

impl<C> SharedCipher<C> {
    fn new(inner: C) -> Self {
        SharedCipher(Arc::new(Mutex::new(inner)))
    }
}

// Hand-written rather than `#[derive(Clone)]`: the derive would add a spurious `C: Clone` bound
// (it clones the whole tuple-struct field-by-field in principle), even though cloning an
// `Arc<Mutex<C>>` only ever bumps the refcount and never needs `C` itself to be `Clone` — and a
// session-key-holding store is exactly the kind of type that should NOT casually implement `Clone`.
impl<C> Clone for SharedCipher<C> {
    fn clone(&self) -> Self {
        SharedCipher(Arc::clone(&self.0))
    }
}

impl<C: SessionCipher> SessionCipher for SharedCipher<C> {
    fn custody(&self) -> KeyCustody {
        self.0.lock().unwrap().custody()
    }
    fn establish(&mut self) -> Result<KeyHandle, SecurityError> {
        self.0.lock().unwrap().establish()
    }
    fn seal(
        &mut self,
        h: KeyHandle,
        aad: &[u8],
        buf: &mut [u8],
        len: usize,
    ) -> Result<usize, SecurityError> {
        self.0.lock().unwrap().seal(h, aad, buf, len)
    }
    fn open(
        &mut self,
        h: KeyHandle,
        aad: &[u8],
        buf: &mut [u8],
        len: usize,
    ) -> Result<usize, SecurityError> {
        self.0.lock().unwrap().open(h, aad, buf, len)
    }
}

fn shared_store() -> SharedCipher<SoftwareSessionStore<InsecureTestRng>> {
    SharedCipher::new(SoftwareSessionStore::new(InsecureTestRng::new(SEED)))
}

fn unit(op: CryptoOp, context: u32, bytes: &[u8]) -> CryptoUnit {
    CryptoUnit {
        op,
        context,
        data: heapless::Vec::from_slice(bytes).unwrap(),
    }
}

#[tokio::test]
async fn seal_then_open_round_trips_across_two_stage_instances() {
    let shared = shared_store();
    let mut seal_stage = CryptoStage::new(shared.clone());
    let mut open_stage = CryptoStage::new(shared.clone());

    let handle = shared.clone().establish().unwrap();

    let plaintext = b"tms/METEOR-001/dc_measurement_update payload";
    let sealed = seal_stage
        .process(unit(CryptoOp::Seal, handle.0, plaintext))
        .await
        .expect("seal must succeed with a freshly established handle");
    // Seal grows the payload (nonce + auth tag framing) — the sealed record is not byte-identical
    // to the plaintext.
    assert_ne!(sealed.data.as_slice(), plaintext);
    assert_eq!(sealed.op, CryptoOp::Seal, "op is carried through unchanged");

    let opened = open_stage
        .process(unit(CryptoOp::Open, handle.0, sealed.data.as_slice()))
        .await
        .expect("open must succeed on an untampered sealed record with the right handle");
    assert_eq!(opened.data.as_slice(), plaintext);
}

#[tokio::test]
async fn tampered_sealed_byte_fails_closed_on_open() {
    let shared = shared_store();
    let mut seal_stage = CryptoStage::new(shared.clone());
    let mut open_stage = CryptoStage::new(shared.clone());
    let handle = shared.clone().establish().unwrap();

    let mut sealed = seal_stage
        .process(unit(
            CryptoOp::Seal,
            handle.0,
            b"do not trust this once flipped",
        ))
        .await
        .unwrap();
    // Flip the last byte (within the auth tag for any reasonable AEAD framing) so the sealed
    // record no longer authenticates.
    let last = sealed.data.len() - 1;
    sealed.data[last] ^= 0xFF;

    let result = open_stage
        .process(unit(CryptoOp::Open, handle.0, sealed.data.as_slice()))
        .await;
    assert_eq!(
        result,
        Err(CryptoStageError::Cipher(SecurityError::VerificationFailed)),
        "a tampered auth tag must fail closed with VerificationFailed"
    );
}

#[tokio::test]
async fn wrong_context_handle_fails_closed_on_open() {
    let shared = shared_store();
    let mut seal_stage = CryptoStage::new(shared.clone());
    let mut open_stage = CryptoStage::new(shared.clone());

    let handle_a = shared.clone().establish().unwrap();
    let handle_b = shared.clone().establish().unwrap();
    assert_ne!(
        handle_a.0, handle_b.0,
        "sanity: two establish() calls must yield distinct handles"
    );

    let sealed = seal_stage
        .process(unit(CryptoOp::Seal, handle_a.0, b"sealed under handle A"))
        .await
        .unwrap();

    // Open the same untampered bytes but under handle B's (wrong) key material.
    let result = open_stage
        .process(unit(CryptoOp::Open, handle_b.0, sealed.data.as_slice()))
        .await;
    assert_eq!(
        result,
        Err(CryptoStageError::Cipher(SecurityError::VerificationFailed)),
        "opening authentic-but-wrong-key ciphertext must fail closed with VerificationFailed"
    );
}

#[tokio::test]
async fn oversize_payload_leaves_no_headroom_and_fails_closed_on_seal() {
    let shared = shared_store();
    let mut seal_stage = CryptoStage::new(shared.clone());
    let handle = shared.clone().establish().unwrap();

    // A payload that already fills the boundary message's fixed capacity leaves zero headroom
    // for seal's nonce + auth-tag framing — this must fail closed, never silently truncate or
    // panic.
    let full = [0u8; CRYPTO_MAX];
    let result = seal_stage
        .process(unit(CryptoOp::Seal, handle.0, &full))
        .await;
    assert_eq!(
        result,
        Err(CryptoStageError::Cipher(SecurityError::BufferTooSmall)),
        "sealing a payload with no framing headroom must fail closed with BufferTooSmall"
    );
}

// --- full `drive()` wiring over a tokio-mpsc channel pair --------------------------------------
//
// Minimal tokio-mpsc adapters to the crate's `Sender`/`Receiver` traits — the same shape as
// `lib_net::link`'s own in-source tests use to drive its async pumps (no generic tokio channel
// impl ships in the library itself; each host consumer wires its own).

struct MpscTx<T>(tokio::sync::mpsc::Sender<T>);
impl<T> Sender for MpscTx<T> {
    type Item = T;
    fn try_send(&mut self, item: T) -> Result<(), TrySendError<T>> {
        self.0.try_send(item).map_err(|e| match e {
            tokio::sync::mpsc::error::TrySendError::Full(i) => TrySendError::Full(i),
            tokio::sync::mpsc::error::TrySendError::Closed(i) => TrySendError::Closed(i),
        })
    }
    async fn send(&mut self, item: T) -> Result<(), Closed> {
        self.0.send(item).await.map_err(|_| Closed)
    }
}
struct MpscRx<T>(tokio::sync::mpsc::Receiver<T>);
impl<T> Receiver for MpscRx<T> {
    type Item = T;
    fn try_recv(&mut self) -> Result<T, TryRecvError> {
        self.0.try_recv().map_err(|e| match e {
            tokio::sync::mpsc::error::TryRecvError::Empty => TryRecvError::Empty,
            tokio::sync::mpsc::error::TryRecvError::Disconnected => TryRecvError::Closed,
        })
    }
    async fn recv(&mut self) -> Option<T> {
        self.0.recv().await
    }
}

#[tokio::test]
async fn drive_wiring_drops_the_failing_item_and_forwards_only_the_good_one() {
    let shared = shared_store();
    let seal_stage = CryptoStage::new(shared.clone());
    let handle = shared.clone().establish().unwrap();

    let (in_tx, in_rx) = tokio::sync::mpsc::channel::<CryptoUnit>(4);
    let (out_tx, out_rx) = tokio::sync::mpsc::channel::<CryptoUnit>(4);

    // A bad item (no framing headroom) followed by a good one, then the input side closes.
    let full = [0u8; CRYPTO_MAX];
    in_tx
        .send(unit(CryptoOp::Seal, handle.0, &full))
        .await
        .unwrap();
    in_tx
        .send(unit(
            CryptoOp::Seal,
            handle.0,
            b"the only one that should come out",
        ))
        .await
        .unwrap();
    drop(in_tx);

    drive(seal_stage, MpscRx(in_rx), MpscTx(out_tx)).await;

    let mut rx = MpscRx(out_rx);
    let only = rx
        .try_recv()
        .expect("the good item must have been sealed and forwarded");
    assert_ne!(only.data.as_slice(), b"the only one that should come out");
    assert_eq!(
        rx.try_recv(),
        Err(TryRecvError::Closed),
        "the bad item must have been dropped, not forwarded — exactly one output item total"
    );
}

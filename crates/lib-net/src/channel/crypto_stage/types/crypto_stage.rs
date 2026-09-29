/// A pipeline [`crate::channel::stage::Stage`] that seals or opens a boundary
/// [`crate::channel::message::CryptoUnit`] in place, riding any [`lib_security::SessionCipher`]
/// implementation — the software AES-GCM store, a wolfCrypt-backed store, or (later) a
/// secure-world store, all interchangeable behind the trait. Generic over `C` so the concrete
/// cipher/backend is a caller choice, never hard-coded here (feature `crypto`).
pub struct CryptoStage<C> {
    /// The session cipher this stage rides. The stage never touches key bytes — only
    /// `KeyHandle`s (derived from `CryptoUnit::context`) and the `SessionCipher::seal`/`open`
    /// seam.
    pub(in super::super) cipher: C,
}

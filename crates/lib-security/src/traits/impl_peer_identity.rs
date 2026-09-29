use super::prelude::*;

impl PeerIdentity {
    /// An empty identity, suitable only as the `out` argument of
    /// [`TrustStore::verify_peer`](super::TrustStore::verify_peer).
    ///
    /// It resolves to no real peer: [`AnchorId`] `0` here is a placeholder, the SPKI hash is all
    /// zeroes, and the subject is empty. It carries **no authorization** — a caller that reads this
    /// value without a preceding `Ok(())` from `verify_peer` has an unverified peer, not an
    /// anonymous one. Authorization is default-deny per anchor (ADR 0013 §6), so an unpopulated
    /// value grants nothing even if it is read by mistake.
    pub fn empty() -> Self {
        PeerIdentity {
            anchor: AnchorId(0),
            spki_sha256: [0u8; 32],
            subject: heapless::String::new(),
        }
    }
}

impl Default for PeerIdentity {
    fn default() -> Self {
        PeerIdentity::empty()
    }
}

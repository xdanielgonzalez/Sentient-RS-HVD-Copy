/// A stable, local identifier for one entry in a [`super::trust_store::TrustStore`] — the thing a
/// successful peer verification resolves *to*, and the key authorization is written against.
///
/// It is deliberately **not** a certificate, a public key, or a hash. Authorization tables
/// (per-anchor publish/subscribe topic-filter sets, see ADR 0013 §6) index by `AnchorId`, so a
/// credential rotation that keeps the same anchor slot keeps the same permissions, and a policy
/// table never has to carry key material.
///
/// The numbering space is **local to one store instance** and meaningless outside it — the same
/// doctrine as [`super::key_handle::KeyHandle`]. Two stores may hand out the same `AnchorId` for
/// unrelated peers; never persist one across a re-provision without re-reading the store.
///
/// See ADR `docs/decisions/0013-transport-security-trust-store.md` §1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AnchorId(pub u16);

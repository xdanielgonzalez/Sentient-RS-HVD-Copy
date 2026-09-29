/// A [`TrustStore`](crate::TrustStore) that trusts **nothing** and verifies **nobody**.
///
/// It holds zero anchors, reports [`TrustModel::Unconfigured`](crate::TrustModel), and every
/// verification and identity lookup fails closed with
/// [`SecurityError::NotImplemented`](crate::SecurityError::NotImplemented). It exists so the seam
/// compiles and can be wired end to end before ADR 0013 WP-2 supplies a real store — and so that
/// wiring it by accident produces a device that refuses every client, not one that accepts every
/// client.
///
/// It is a zero-sized unit struct: constructing it costs nothing and grants nothing.
#[derive(Debug, Clone, Copy, Default)]
pub struct UnimplementedTrustStore;

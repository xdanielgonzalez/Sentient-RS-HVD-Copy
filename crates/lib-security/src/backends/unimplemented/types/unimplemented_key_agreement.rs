/// A [`KeyAgreement`](crate::KeyAgreement) that agrees on nothing.
///
/// It cannot generate an ephemeral key, cannot produce a public share, and cannot derive a shared
/// secret — every method fails closed with
/// [`SecurityError::NotImplemented`](crate::SecurityError::NotImplemented). Crucially it never
/// hands back a [`KeyHandle`](crate::KeyHandle): a fabricated handle would flow into
/// [`KeyDerivation`](crate::KeyDerivation) and produce a key schedule over a secret that does not
/// exist, which is precisely the failure a stub must not be able to cause.
///
/// ADR 0013 WP-2 replaces it with a `p256`/`ecdh` implementation.
#[derive(Debug, Clone, Copy, Default)]
pub struct UnimplementedKeyAgreement;

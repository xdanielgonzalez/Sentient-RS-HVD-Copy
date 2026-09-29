/// An elliptic-curve group for ephemeral key agreement — the `supported_groups` vocabulary of a
/// TLS 1.3 handshake, restricted to the EC-only profile ADR 0013 §3 selects.
///
/// The ADR names the parameter but not its variants; this is the minimal set consistent with an
/// EC-only, TLS-1.3-only profile whose signature vocabulary is already
/// [`Algorithm::EcdsaP256Sha256`](super::algorithm::Algorithm::EcdsaP256Sha256) /
/// [`Algorithm::EcdsaP384Sha384`](super::algorithm::Algorithm::EcdsaP384Sha384). It is a *separate*
/// enum from [`Algorithm`](super::algorithm::Algorithm) because agreement groups and signature
/// algorithms are independently negotiated in TLS 1.3 and independently approved under FIPS —
/// collapsing them would make one gate answer for both.
///
/// `#[non_exhaustive]`: adding a group (e.g. a hybrid post-quantum one) must not silently compile
/// into an existing `match` arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum KexGroup {
    /// NIST P-256 (secp256r1), uncompressed point shares.
    NistP256,
    /// NIST P-384 (secp384r1), uncompressed point shares.
    NistP384,
}

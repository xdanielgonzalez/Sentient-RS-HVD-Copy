/// Digest algorithm identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Digest {
    /// SHA-256 (approved).
    Sha256,
    /// SHA-384 (approved).
    Sha384,
    /// SHA-512 (approved).
    Sha512,
}

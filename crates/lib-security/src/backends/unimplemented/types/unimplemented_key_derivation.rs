/// A [`KeyDerivation`](crate::KeyDerivation) that derives nothing.
///
/// Extract, expand-to-handle and expand-to-bytes all fail closed with
/// [`SecurityError::NotImplemented`](crate::SecurityError::NotImplemented). In particular
/// `expand_to_bytes` never writes a byte into the caller's buffer: a stub that filled it with
/// zeroes would hand a caller an all-zero "key" it might use.
///
/// ADR 0013 WP-2 replaces it with an HKDF implementation.
#[derive(Debug, Clone, Copy, Default)]
pub struct UnimplementedKeyDerivation;

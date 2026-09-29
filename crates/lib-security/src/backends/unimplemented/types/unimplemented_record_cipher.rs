/// A [`RecordCipher`](crate::RecordCipher) that seals nothing and opens nothing.
///
/// Both directions fail closed with
/// [`SecurityError::NotImplemented`](crate::SecurityError::NotImplemented) and neither touches the
/// caller's buffer. The two failure modes this makes impossible are the ones that matter: a
/// `seal_record` that returned `Ok` while leaving plaintext in the buffer would transmit plaintext
/// on a socket the caller believes is encrypted, and an `open_record` that returned `Ok` would hand
/// unauthenticated attacker bytes to the broker as verified plaintext.
///
/// ADR 0013 WP-2 replaces it with an AES-GCM implementation that enforces per-(key, direction)
/// sequence monotonicity.
#[derive(Debug, Clone, Copy, Default)]
pub struct UnimplementedRecordCipher;

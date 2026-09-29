/// A minimal signed-claims [`TokenVerifier`] over ECDSA P-256 — see the module doc for the wire
/// format. Stateless: `trusted_key` (the issuer's raw SEC1 public key) is supplied per call, as the
/// trait requires.
///
/// [`TokenVerifier`]: crate::traits::TokenVerifier
#[derive(Debug, Clone, Copy, Default)]
pub struct SoftwareTokenVerifier;

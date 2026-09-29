use heapless::Vec as HVec;
use p256::ecdsa::SigningKey;

use crate::traits::Rng;

/// Max keys a [`SoftwareKeyStore`] holds at once (bounded, no allocation).
pub const ARENA_CAP: usize = 8;

/// [`crate::KeyStore`] over a bounded in-memory arena of ECDSA P-256 keys — the SOFTWARE tier's concrete
/// key custody. Only [`crate::Algorithm::EcdsaP256Sha256`] is implemented — see the parent module's doc for
/// why. Any other requested algorithm is refused with [`crate::SecurityError::NotImplemented`] (fail
/// closed).
///
/// `Debug` (required by consumers like `lib_link::tls_identity::KeyStoreTlsIdentity`, which
/// bounds its `KeyStore` type parameter on it) never leaks key material: `ecdsa::SigningKey`'s own
/// `Debug` impl is `finish_non_exhaustive()` — no fields printed.
#[derive(Debug)]
pub struct SoftwareKeyStore<R: Rng> {
    pub(in crate::backends::software::keystore) rng: R,
    pub(in crate::backends::software::keystore) keys: HVec<SigningKey, ARENA_CAP>,
}

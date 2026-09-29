use super::algorithm::Algorithm;
use super::key_handle::KeyHandle;
use crate::error::Result;
use crate::profile::KeyCustody;

/// Custody of private keys. There is **no** method to export a private key — signing happens *at*
/// the store, so a secure-world implementation keeps the key in hardware and a software
/// implementation still cannot expose it through this trait.
pub trait KeyStore {
    /// Which custody tier this store actually provides. **Mandatory honesty accessor:** a software
    /// store MUST return [`KeyCustody::SoftwareMemory`] so callers, logs, and customer-facing claims
    /// can never mistake it for hardware-bound custody (the top false-security risk in the
    /// integration map). A software key is exfiltrable by a compromised rootfs; a secure-world key
    /// is not — the API forces that distinction to be visible.
    fn custody(&self) -> KeyCustody;
    /// Generate a new key for `algorithm`; returns an opaque handle. The private key never leaves
    /// the store.
    fn generate(&mut self, algorithm: Algorithm) -> Result<KeyHandle>;
    /// Import a private key into the store's custody from `bytes`, returning a handle. (Software
    /// tier only in practice; a secure-world store may refuse import of extractable material.)
    fn import_private(&mut self, algorithm: Algorithm, bytes: &[u8]) -> Result<KeyHandle>;
    /// Write the *public* key for `handle` into `out`. Public material only — never private.
    fn export_public(&self, handle: KeyHandle, out: &mut [u8]) -> Result<usize>;
    /// Sign `message` with the key referenced by `handle`, writing the signature into `out`.
    fn sign(&self, handle: KeyHandle, message: &[u8], out: &mut [u8]) -> Result<usize>;
}

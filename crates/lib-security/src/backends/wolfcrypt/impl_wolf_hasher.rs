use super::prelude::*;
use super::types::WolfHasher;

impl WolfHasher {
    /// Start a hasher for `digest`.
    pub fn new(digest: Digest) -> Self {
        // NOTE: `<wolf::ShaNNN>::new()` is ambiguous — `crate::sha::SHAxxx` has BOTH an inherent
        // `fn new() -> Result<Self, i32>` (the native wolfCrypt constructor) AND, via the
        // `sha_digest.rs` blanket impl, `digest::Digest::new() -> Self` (infallible — backed by
        // `Default::default()`, which `.expect()`s the native constructor). Inherent methods shadow
        // trait methods for `Type::method()` syntax, so we must disambiguate to reach the trait
        // method; that is the one this seam is written against (see `finish_into`/`update` below,
        // which likewise go through the `digest::Digest` trait surface, not the native API).
        let state = match digest {
            Digest::Sha256 => WolfDigestState::Sha256(<wolf::Sha256 as _RcDigest>::new()),
            Digest::Sha384 => WolfDigestState::Sha384(<wolf::Sha384 as _RcDigest>::new()),
            Digest::Sha512 => WolfDigestState::Sha512(<wolf::Sha512 as _RcDigest>::new()),
        };
        WolfHasher {
            which: digest,
            state,
        }
    }
}

impl Hasher for WolfHasher {
    fn digest(&self) -> Digest {
        self.which
    }

    fn update(&mut self, data: &[u8]) {
        // NOTE: `h.update(data)` would be ambiguous/wrong here — `crate::sha::SHAxxx` ALSO has an
        // inherent `fn update(&mut self, data: &[u8]) -> Result<(), i32>` (the native constructor,
        // whose `Result` would be silently dropped by method-call syntax preferring the inherent
        // method over the `digest::Update`/`Digest` trait method). Disambiguate explicitly to reach
        // the infallible trait method (matching `finalize_reset` below), and to keep a native update
        // failure from being silently swallowed rather than surfaced.
        match &mut self.state {
            WolfDigestState::Sha256(h) => _RcDigest::update(h, data),
            WolfDigestState::Sha384(h) => _RcDigest::update(h, data),
            WolfDigestState::Sha512(h) => _RcDigest::update(h, data),
        }
    }

    fn finish(&mut self, out: &mut [u8]) -> Result<usize> {
        match &mut self.state {
            WolfDigestState::Sha256(h) => finish_into(h, out),
            WolfDigestState::Sha384(h) => finish_into(h, out),
            WolfDigestState::Sha512(h) => finish_into(h, out),
        }
    }
}

/// Finalize `hasher` in place (via `Digest::finalize_reset`, which resets the hasher rather than
/// consuming it, so `finish` need not consume `self.state`) and copy the digest into `out`.
/// `BufferTooSmall` if `out` cannot hold the whole digest — bounded, no allocation.
///
/// NOTE: this binds to `FixedOutputReset` rather than requiring `Clone` (the original seam design) —
/// `wolfssl_wolfcrypt::sha::{SHA256, SHA384, SHA512}` wrap a raw wolfSSL C hash-context struct and do
/// NOT derive `Clone`/`Copy`. `finalize_reset` is the RustCrypto-idiomatic non-consuming finalize and
/// is available here because `sha_digest.rs` implements `FixedOutputReset` for these types.
fn finish_into<D>(hasher: &mut D, out: &mut [u8]) -> Result<usize>
where
    D: _RcDigest + _RcFixedOutputReset,
{
    let digest = hasher.finalize_reset();
    let bytes: &[u8] = digest.as_ref();
    if out.len() < bytes.len() {
        return Err(SecurityError::BufferTooSmall);
    }
    out[..bytes.len()].copy_from_slice(bytes);
    Ok(bytes.len())
}

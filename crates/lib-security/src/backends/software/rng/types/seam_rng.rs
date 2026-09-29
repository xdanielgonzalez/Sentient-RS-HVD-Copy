use crate::traits::Rng;

/// Adapts this crate's fallible [`Rng`] seam to `rand_core::TryRng`/`TryCryptoRng`. Zero-alloc:
/// `try_next_u32`/`try_next_u64` are implemented over small stack buffers via `try_fill_bytes`.
pub struct SeamRng<'a, R: Rng + ?Sized>(pub &'a mut R);

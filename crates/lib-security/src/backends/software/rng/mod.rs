//! Entropy for the software backend: the seam adapter [`generate`](super::keystore) draws from, and
//! a deterministic test/demo double.
//!
//! [`SeamRng`] adapts this crate's fallible [`Rng`] seam to `rand_core::TryRng`/`TryCryptoRng` — the
//! bound `elliptic_curve::Generate::try_generate_from_rng` requires
//! (`super::keystore::SoftwareKeyStore::generate` is the only caller). Binding to the seam's `Rng` —
//! not `p256`'s own optional `getrandom` feature — keeps entropy sourcing swappable, testable, and
//! injectable, matching [`super::super::wolfcrypt::WolfRng`]'s rationale for its own native
//! (non-`rand_core`) RNG binding.
//!
//! [`InsecureTestRng`] is a non-cryptographic, deterministic `Rng` for tests/examples ONLY.

mod prelude;
mod types;

pub use types::*;

mod impl_insecure_test_rng;
mod impl_seam_rng;

#[cfg(test)]
use prelude::*;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_is_deterministic() {
        let mut a = InsecureTestRng::new(1234);
        let mut b = InsecureTestRng::new(1234);
        let mut out_a = [0u8; 37]; // deliberately not a multiple of 8, exercises the tail copy
        let mut out_b = [0u8; 37];
        a.fill(&mut out_a).unwrap();
        b.fill(&mut out_b).unwrap();
        assert_eq!(out_a, out_b, "same seed must reproduce identical output");
    }

    #[test]
    fn different_seeds_diverge() {
        let mut a = InsecureTestRng::new(1);
        let mut b = InsecureTestRng::new(2);
        let mut out_a = [0u8; 32];
        let mut out_b = [0u8; 32];
        a.fill(&mut out_a).unwrap();
        b.fill(&mut out_b).unwrap();
        assert_ne!(out_a, out_b);
    }

    #[test]
    fn fill_never_leaves_a_short_buffer_partially_written() {
        let mut rng = InsecureTestRng::new(7);
        let mut out = [0u8; 1];
        rng.fill(&mut out).unwrap();
        // Just needs to not panic/underflow on a buffer shorter than one u64.
    }
}

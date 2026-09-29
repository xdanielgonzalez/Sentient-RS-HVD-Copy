/// A deterministic, **NON-CRYPTOGRAPHIC** `Rng` for tests and examples ONLY (a splitmix64 step).
/// The same seed yields IDENTICAL output on every run — exactly what a real entropy source must
/// NEVER do. Never construct this outside test/demo code; production callers must supply a real
/// entropy source (OS CSPRNG, hardware TRNG) through the [`crate::Rng`] seam.
#[derive(Debug, Clone)]
pub struct InsecureTestRng(pub u64);

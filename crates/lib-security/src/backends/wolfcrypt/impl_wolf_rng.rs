use super::prelude::*;
use super::types::WolfRng;

impl WolfRng {
    /// Construct the wolfCrypt RNG. A failure to instantiate/seed the DRBG is an
    /// [`SecurityError::EntropyFailure`] (fail closed — never a silently-unseeded generator).
    pub fn new() -> Result<Self> {
        // WOLFSSL-API: confirmed — `random::RNG::new()` wraps `wc_InitRng`, returning
        // `Result<Self, i32>` (the wolfSSL library error code). Map any failure to EntropyFailure;
        // do NOT expose an unseeded generator.
        let inner = wolf::Rng::new().map_err(|_| SecurityError::EntropyFailure)?;
        Ok(WolfRng { inner })
    }
}

impl Rng for WolfRng {
    fn fill(&mut self, out: &mut [u8]) -> Result<()> {
        // WOLFSSL-API: confirmed — `RNG::generate_block` wraps `wc_RNG_GenerateBlock` and returns
        // `Result<(), i32>` natively. We deliberately do NOT go through `rand_core::TryRng` here: its
        // `Error = Infallible` and the crate's own `try_fill_bytes` impl `.expect()`s success on a
        // wolfCrypt draw failure (a panic), which would violate this crate's fail-closed contract. A
        // DRBG health failure / unavailable entropy therefore maps to EntropyFailure via this native
        // call, never a panic and never silent weak output.
        self.inner
            .generate_block(out)
            .map_err(|_| SecurityError::EntropyFailure)
    }
}

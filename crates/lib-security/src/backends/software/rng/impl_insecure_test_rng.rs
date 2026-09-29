use super::prelude::*;
use super::types::InsecureTestRng;

impl InsecureTestRng {
    /// A deterministic generator seeded by `seed`. Same `seed` -> same key material, every time.
    pub fn new(seed: u64) -> Self {
        InsecureTestRng(seed)
    }

    fn next_u64(&mut self) -> u64 {
        // splitmix64 — fast, well-distributed, NOT cryptographically secure.
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

impl Rng for InsecureTestRng {
    fn fill(&mut self, out: &mut [u8]) -> Result<()> {
        let mut i = 0;
        while i < out.len() {
            let bytes = self.next_u64().to_le_bytes();
            let n = core::cmp::min(bytes.len(), out.len() - i);
            out[i..i + n].copy_from_slice(&bytes[..n]);
            i += n;
        }
        Ok(())
    }
}

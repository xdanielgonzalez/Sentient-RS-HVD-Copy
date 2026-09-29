use super::prelude::*;
use super::types::SeamRng;

impl<R: Rng + ?Sized> RcTryRng for SeamRng<'_, R> {
    type Error = SecurityError;

    fn try_next_u32(&mut self) -> core::result::Result<u32, Self::Error> {
        let mut buf = [0u8; 4];
        self.0.fill(&mut buf)?;
        Ok(u32::from_le_bytes(buf))
    }

    fn try_next_u64(&mut self) -> core::result::Result<u64, Self::Error> {
        let mut buf = [0u8; 8];
        self.0.fill(&mut buf)?;
        Ok(u64::from_le_bytes(buf))
    }

    fn try_fill_bytes(&mut self, dst: &mut [u8]) -> core::result::Result<(), Self::Error> {
        self.0.fill(dst)
    }
}

// Marker trait, no methods of its own — `Rng::fill` is documented (`crate::traits::Rng`) as drawing
// cryptographically secure random bytes, so a `SeamRng` over it qualifies.
impl<R: Rng + ?Sized> RcTryCryptoRng for SeamRng<'_, R> {}

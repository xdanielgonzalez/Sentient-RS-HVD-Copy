use crate::prelude::*;

impl ErrorType for Rp2350MeshDe {
    /// `Infallible` already implements `embedded_hal::digital::Error` (a blanket impl in
    /// `embedded-hal` 1.0's `digital` module) — this no-op pin can never fail, so there is nothing
    /// else worth naming here.
    type Error = core::convert::Infallible;
}

impl OutputPin for Rp2350MeshDe {
    fn set_low(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
    fn set_high(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

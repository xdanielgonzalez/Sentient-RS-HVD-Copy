use crate::prelude::*;

impl<I2C> RailMonitor for Ina3221RailMonitor<I2C>
where
    I2C: I2c,
{
    /// Delegates straight to the wrapped driver's own `read_all` (`lib_io::ina3221::Ina3221`'s
    /// inherent method, already written to this exact shape — see that module's doc comment). This
    /// impl exists purely to close the seam gap: a caller reaching through [`RailMonitor`] gets
    /// static dispatch onto the real driver, rather than every caller needing to know the concrete
    /// `Ina3221<I2C>` type.
    async fn read_all(&mut self, out: &mut [DeviceRailReading]) -> usize {
        self.0.read_all(out).await
    }
}

impl<I2C> core::ops::Deref for Ina3221RailMonitor<I2C> {
    type Target = Ina3221<I2C>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<I2C> core::ops::DerefMut for Ina3221RailMonitor<I2C> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

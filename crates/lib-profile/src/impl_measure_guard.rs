#[cfg(feature = "profile")]
use crate::types::MeasureGuard;

#[cfg(feature = "profile")]
impl<'a, const N: usize> Drop for MeasureGuard<'a, N> {
    fn drop(&mut self) {
        let end_micros = self.clock.now_micros();
        let elapsed = end_micros.saturating_sub(self.start_micros);
        self.registry.record_span(self.name, elapsed);
    }
}

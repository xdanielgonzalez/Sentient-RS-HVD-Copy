use lib_mapping::schema::ClockMonotonic;

/// The injected clock/sleep seam that keeps this crate embassy-version-free (see the crate root
/// doc comment): [`super::McuPlatform`] is generic over `C: McuClock`, so `bin/mcu-rp2350` supplies its
/// own concrete `embassy_time`-backed clock (whichever `embassy-time` generation IT pins) without
/// this crate ever naming `embassy-time` itself.
#[allow(async_fn_in_trait)]
pub trait McuClock {
    /// A monotonic timestamp for TMS payloads.
    fn now(&self) -> ClockMonotonic;
    /// Sleep `ms` milliseconds between poll cycles.
    async fn sleep_ms(&mut self, ms: u32);
}

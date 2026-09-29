use embassy_time::Instant;

/// Wall/monotonic clock for the RP2350 NS world — implements `bin_mcu_platform::McuClock`
/// (`impl_embassy_clock.rs`) over `embassy_time`. `boot` captures the instant this clock was
/// constructed (once, at startup, before the poll loop spawns); `now()` reports elapsed time since
/// then. Same "uptime since boot" contract `bin_zephyr_platform::ZephyrPlatform::now` used over
/// `sz_uptime_ms()` on the retired Zephyr path.
pub struct EmbassyClock {
    pub(crate) boot: Instant,
}

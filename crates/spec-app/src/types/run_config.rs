use crate::MAX_DIPS;
use lib_mapping::schema::Identity;

/// Platform-neutral run parameters (from config). Bounded, `no_std`-friendly.
///
/// Every field is bounded (`heapless`, no alloc), so construction is fallible only at the
/// capacity limits — the `?`s below surface a too-long id/prefix or too many DIP addresses:
///
/// ```rust
/// use lib_mapping::schema::Identity;
/// use spec_app::RunConfig;
///
/// # fn main() -> Result<(), heapless::CapacityError> {
/// let cfg = RunConfig {
///     device_id: Identity::try_from("METEOR-001")?,
///     prefix: heapless::String::try_from("tms")?,
///     dip_addresses: heapless::Vec::from_slice(&[0x00, 0x01])?,
///     interval_ms: 2_000,
///     fault_every: 10,
///     max_backoff_ms: 30_000,
/// };
///
/// assert_eq!(cfg.device_id.as_str(), "METEOR-001");
/// assert_eq!(cfg.dip_addresses.len(), 2);
/// // Fault polling every 10th cycle at a 2 s cadence = one 0x44 exchange per 20 s.
/// assert_eq!(cfg.interval_ms * cfg.fault_every, 20_000);
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct RunConfig {
    /// Device identity published on the TMS topics.
    pub device_id: Identity,
    /// Topic prefix (e.g. `tms`).
    pub prefix: heapless::String<16>,
    /// Seplos DIP addresses polled each cycle.
    pub dip_addresses: heapless::Vec<u8, MAX_DIPS>,
    /// Cycle cadence in milliseconds.
    pub interval_ms: u32,
    /// Poll the `0x44` fault command every N cycles (0 disables).
    pub fault_every: u32,
    /// Ceiling (in milliseconds) for the dead-cycle backoff ramp (1 s start, exponential up to
    /// this cap). Platform-tunable: a truly-absent BMS on the Pi shouldn't be hammered (30 s), but
    /// the MCU demo wants a tight retry so telemetry recovers fast from a transient UART fault
    /// (e.g. 2 s).
    pub max_backoff_ms: u32,
}

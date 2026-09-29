use serde::Deserialize;

/// Slowest `[poll].interval_ms` that still meets `DcMeasurementUpdate`'s MIL-STD-3071 nominal
/// publish rate. See [`PollConfig::validate`] for why this bound exists.
pub const MAX_INTERVAL_MS_FOR_1HZ: u64 = 1000;

/// Highest encodable Seplos DIP address: the protocol packs the address into one ASCII-hex digit
/// (`'0'..='9','A'..='F'`, i.e. 4 bits — see `lib_parse::seplos::command`'s doc comment and
/// `SeplosError::AddrOutOfRange`). A `dip_addresses` entry above this bound would parse fine here
/// but fail every single poll cycle forever once the request is actually encoded — see
/// [`PollConfig::validate`].
pub const MAX_SEPLOS_DIP_ADDRESS: u8 = 15;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PollConfig {
    /// DIP addresses to poll each cycle. Must be non-empty (see [`PollConfig::validate`] — an
    /// empty list is a device configured to poll nothing) and each entry `0..=`
    /// [`MAX_SEPLOS_DIP_ADDRESS`] (the Seplos-protocol-encodable range; unused/harmless on a
    /// Voltronic bus, which never reads this field, but bounded uniformly since `PollConfig` is
    /// shared across both).
    pub dip_addresses: Vec<u8>,
    /// Cycle cadence in milliseconds. Must be `1..=`[`MAX_INTERVAL_MS_FOR_1HZ`] — see
    /// [`PollConfig::validate`].
    pub interval_ms: u64,
    /// Poll the fault command every N cycles (0 disables fault polling).
    pub fault_every: u32,
}

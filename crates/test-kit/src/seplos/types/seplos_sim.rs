use super::battery_state::BatteryState;
use super::behavior::Behavior;

/// The pure Seplos v2 responder. Owns a [`BatteryState`] to encode from and a [`Behavior`]
/// controlling how the next request is answered.
#[derive(Debug, Clone)]
pub struct SeplosSim {
    pub(in crate::seplos) state: BatteryState,
    pub(in crate::seplos) behavior: Behavior,
    /// Opt-in (default off) auto-NAK mode: see [`SeplosSim::set_auto_nak`].
    pub(in crate::seplos) auto_nak: bool,
}

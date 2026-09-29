use super::prelude::*;

impl SeplosSim {
    /// Construct a responder over `state`, starting in [`Behavior::Normal`].
    pub fn new(state: BatteryState) -> Self {
        SeplosSim {
            state,
            behavior: Behavior::Normal,
            auto_nak: false,
        }
    }

    /// Read-only access to the battery state being encoded.
    pub fn state(&self) -> &BatteryState {
        &self.state
    }

    /// Mutable access, e.g. to flip a fault flag or change cell voltages between exchanges.
    pub fn state_mut(&mut self) -> &mut BatteryState {
        &mut self.state
    }

    /// Set how the next (and subsequent, until changed again) [`respond`](Self::respond) calls
    /// answer.
    pub fn set_behavior(&mut self, behavior: Behavior) {
        self.behavior = behavior;
    }

    /// Enable/disable auto-NAK mode. Off by default (preserving every existing
    /// "malformed request -> Silence" test unchanged); when on, a request that fails
    /// [`respond`](Self::respond)'s normal parse for one of three protocol-recognizable reasons —
    /// a bad frame CHKSUM, a bad LENGTH LCHKSUM, or an unrecognized CID2 — gets a
    /// protocol-correct RTN error frame instead of silence, the way a real device answers a
    /// malformed-but-addressable request.
    pub fn set_auto_nak(&mut self, enabled: bool) {
        self.auto_nak = enabled;
    }
}

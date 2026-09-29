use super::prelude::*;

impl SeplosResponder {
    /// Construct a responder over the given canned battery state.
    pub fn new(state: BatteryState) -> Self {
        SeplosResponder { state }
    }
}

use super::BatteryState;

/// A pure Seplos telemetry (`0x42`) responder over a fixed [`BatteryState`]. No I/O of its own —
/// see `seplos_responder/mod.rs`'s module doc. Trimmed from `test-kit`'s `SeplosSim`: no
/// `Behavior`/auto-NAK fields (phase 1 is Normal-behavior-only). Constructor lives in
/// `impl_responder.rs`, the `respond` method in `impl_respond.rs` (type/impl split convention —
/// see the repo's CLAUDE.md "Code Organization").
#[derive(Debug, Clone)]
pub struct SeplosResponder {
    pub state: BatteryState,
}

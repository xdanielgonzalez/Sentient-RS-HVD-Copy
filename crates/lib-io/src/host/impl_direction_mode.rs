use super::prelude::*;

impl Default for DirectionMode {
    /// Software RTS toggling, TX-drained before dropping DE — today's historical behavior PLUS the
    /// drain/settle fix this workstream adds (see the type's doc comment). The settle defaults
    /// (5ms assert-to-write, 10ms write-to-deassert) mirror `bin-rpi`'s `[serial]` config defaults
    /// (`settle_tx_us = 5000`, `settle_rx_us = 10000`) so `DirectionMode::default()` and an
    /// unconfigured TOML `[serial]` section agree.
    fn default() -> Self {
        DirectionMode::RtsSoftware {
            drain: true,
            settle_tx_us: 5_000,
            settle_rx_us: 10_000,
        }
    }
}

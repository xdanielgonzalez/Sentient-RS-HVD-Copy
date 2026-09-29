/// A raw L2 link failure. Fail-closed; no panics.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkError {
    /// The link is down / closed — the pump should stop.
    Down,
    /// A frame was dropped (RX overrun / TX ring full) — transient, keep going.
    Overrun,
    /// Underlying device/driver error.
    Device,
}

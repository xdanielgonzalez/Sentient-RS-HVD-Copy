//! The channel abstraction: a bounded, typed, one-directional link between two stages.
//!
//! Fail-closed and back-pressured: a full channel refuses (`Full`) rather than dropping silently or
//! growing unbounded, so a slow/stuck downstream stage cannot exhaust memory — it applies
//! back-pressure the upstream stage must handle. Implementations map this to an embassy channel
//! (MCU tasks), a tokio mpsc (host tasks), or an IPC transport (isolated processes) — the stage
//! code depends only on these traits, never the concrete queue.

mod types;

// The embassy-sync 0.6.2 `Sender`/`Receiver` impls (feature `channel-embassy`, no_std MCU) — the
// concrete channel `bin-mcu-platform`'s `ChannelPublisher` is generic over. See that module's doc
// comment for the never-closes mapping and why this feature must never pull in
// `embassy-net`/`embassy-time` (the `bin/mcu-rp2350` excluded-workspace version conflict).
#[cfg(feature = "channel-embassy")]
mod impl_embassy_sync;

pub use types::*;

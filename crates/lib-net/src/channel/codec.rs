//! Strict packed-byte codec for boundary messages (postcard), for the IPC / process-isolated
//! channel transports.
//!
//! postcard is a compact, `no_std`, schema-strict binary format — the message's Rust type IS the
//! schema, so decode fails closed on anything that doesn't match (no loose/self-describing parsing
//! like JSON). Encode into a caller-provided buffer (no heap); decode borrows from a byte slice.
//! Available under `feature = "packed"`.

use crate::channel::channel::TryRecvError;
use serde::{Deserialize, Serialize};

/// A packed-codec failure. Distinct from channel errors; fail-closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecError {
    /// Output buffer too small for the encoded message.
    BufferTooSmall,
    /// Input bytes did not decode to this message type (truncated/malformed/schema mismatch).
    Malformed,
}

impl From<CodecError> for TryRecvError {
    fn from(_: CodecError) -> Self {
        // A malformed framed message reads as "nothing valid to receive".
        TryRecvError::Empty
    }
}

/// Encode `msg` into `out` as packed bytes; returns the used prefix length. Fail-closed on overflow.
pub fn encode<T: Serialize>(msg: &T, out: &mut [u8]) -> Result<usize, CodecError> {
    postcard::to_slice(msg, out)
        .map(|used| used.len())
        .map_err(|_| CodecError::BufferTooSmall)
}

/// Decode a packed message from `bytes`. Fail-closed on any mismatch.
pub fn decode<'a, T: Deserialize<'a>>(bytes: &'a [u8]) -> Result<T, CodecError> {
    postcard::from_bytes(bytes).map_err(|_| CodecError::Malformed)
}

use heapless::Vec;

use super::super::MAX_FRAME;

/// [`crate::seplos_responder::SeplosResponder::respond`]'s output: either bytes to write to the
/// wire, or nothing. No `alloc`, so there is no `Box` to shrink `Silence` into — the bytes live
/// inline in a bounded `heapless::Vec`. Ported verbatim from `test-kit`'s `SimReply`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum SimReply {
    /// Bytes to send back verbatim.
    Bytes(Vec<u8, MAX_FRAME>),
    /// Send nothing (an unrecognized or malformed request — a real bus stays silent, it never
    /// fabricates a reply).
    Silence,
}

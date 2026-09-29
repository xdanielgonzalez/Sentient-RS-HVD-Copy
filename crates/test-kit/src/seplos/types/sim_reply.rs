use heapless::Vec;

use super::super::MAX_FRAME;

/// [`crate::seplos::SeplosSim::respond`]'s output: either bytes to write to the wire, or nothing.
///
/// The size gap between variants is deliberate: no `alloc`, so there is no `Box` to shrink
/// `Silence` into — the bytes live inline in a bounded `heapless::Vec`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum SimReply {
    /// Bytes to send back verbatim.
    Bytes(Vec<u8, MAX_FRAME>),
    /// Send nothing (silent bus).
    Silence,
}

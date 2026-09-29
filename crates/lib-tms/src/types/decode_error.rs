/// Error returned by `crate::generated::TmsMessage::decode` (and `encode`) — the host-only serde
/// factory. Kept payload-free so the type itself compiles in `no_std` (the serde-json machinery it
/// wraps is what actually needs `alloc`/`std`, hence the methods — not this enum — are feature-gated).
///
/// # Example
///
/// The factory's first step is topic dispatch: a string with no
/// [`crate::generated::TmsTopicId`] maps to [`DecodeError::UnknownTopic`] — reproducible in
/// `no_std`, without the `std`-gated factory itself:
///
/// ```rust
/// use lib_tms::{DecodeError, TmsTopicId};
///
/// let id = TmsTopicId::from_str("NotATmsTopic").ok_or(DecodeError::UnknownTopic);
/// assert_eq!(id, Err(DecodeError::UnknownTopic));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    /// The topic string did not match any known TMS topic (no [`crate::generated::TmsTopicId`]).
    UnknownTopic,
    /// The bytes were not valid JSON for the topic's concrete type.
    Json,
}

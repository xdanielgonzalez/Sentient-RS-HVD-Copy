/// Mesh-radio I/O error.
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum RadioError {
    #[error("mesh radio serial error: {0}")]
    Io(String),
    #[error("mesh radio send timed out")]
    Timeout,
    /// The hand-encoded on-air frame did not fit in the caller's send buffer (see
    /// [`crate::mesh::MAX_RADIO_FRAME`]).
    #[error("mesh radio frame too large for the {0}-byte send buffer")]
    FrameTooLarge(usize),
}

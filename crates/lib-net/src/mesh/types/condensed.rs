/// The output of [`crate::mesh::condense`]: the condensed on-air bytes plus whether the input had to be
/// truncated to fit `max`.
#[derive(Debug, Clone, Default)]
pub struct Condensed {
    /// The condensed payload bytes, ready to hand to [`crate::mesh::run_radio`].
    pub payload: Vec<u8>,
    /// True if `max` was reached before every available field could be included.
    pub truncated: bool,
}

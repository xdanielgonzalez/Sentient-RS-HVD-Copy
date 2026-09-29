/// Exponential backoff for dead cycles: 1 s doubling to a configurable cap (30 s by default via
/// [`Backoff::new`]; a platform can tighten the ceiling with [`Backoff::with_max`]). Bounded
/// `u32` millis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Backoff {
    pub(crate) cur_ms: u32,
    pub(crate) max_ms: u32,
}

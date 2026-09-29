//! A single named span entry in the registry (internal type).

use core::sync::atomic::AtomicUsize;

/// A single named span entry in the registry.
pub(crate) struct SpanEntry {
    pub(crate) name: &'static str,
    pub(crate) count: AtomicUsize,
    pub(crate) min_micros: AtomicUsize,
    pub(crate) max_micros: AtomicUsize,
    pub(crate) sum_micros: AtomicUsize,
}

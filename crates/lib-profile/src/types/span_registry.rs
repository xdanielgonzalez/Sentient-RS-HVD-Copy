//! Fixed-capacity span registry type.

use super::span_entry::SpanEntry;
use core::cell::UnsafeCell;
use heapless::Vec;

/// Fixed-capacity span registry.
///
/// Stores up to N named spans; each span tracks count, min, max, and sum of durations.
///
/// # Concurrency Model
///
/// Uses lock-free atomic operations for concurrent access. This is safe for the RP2350 MCU
/// context where:
/// - The Embassy executor is cooperative (no preemption mid-function)
/// - `measure_block!` calls complete without awaiting
/// - Each span's min/max/sum updates via CAS loops on atomics with Acquire/Release ordering
///
/// Multiple Embassy tasks can safely call `record_span` concurrently since compare-exchange
/// loops ensure atomicity and there are no long-held locks that could deadlock or starve.
pub struct SpanRegistry<const N: usize> {
    pub(crate) entries: UnsafeCell<Vec<SpanEntry, N>>,
}

// SAFETY: entries are guarded by atomics and Relaxed/Acquire/Release orderings.
// Multiple tasks can call record_span concurrently without preemption concerns.
unsafe impl<const N: usize> Send for SpanRegistry<N> {}
unsafe impl<const N: usize> Sync for SpanRegistry<N> {}

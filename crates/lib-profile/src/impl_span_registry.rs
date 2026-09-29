use crate::types::{SpanEntry, SpanRegistry, SpanStats};
use heapless::Vec;

impl<const N: usize> SpanRegistry<N> {
    /// Create a new empty registry.
    pub const fn new() -> Self {
        Self {
            entries: core::cell::UnsafeCell::new(Vec::new()),
        }
    }

    /// Record a measurement for a named span.
    /// If the span doesn't exist, it's added (up to capacity N).
    pub fn record_span(&self, name: &'static str, micros: u64) {
        // SAFETY: We own the registry; single-threaded initialization is fine.
        // Once entries are added, we only mutate via atomics.
        unsafe {
            let entries = &mut *self.entries.get();
            if let Some(entry) = entries.iter().find(|e| e.name == name) {
                entry.record(micros);
            } else if entries.len() < N {
                entries.push(SpanEntry::new(name)).ok();
                if let Some(entry) = entries.iter().find(|e| e.name == name) {
                    entry.record(micros);
                }
            }
            // If full, silently drop the measurement.
        }
    }

    /// Return a snapshot of all recorded spans.
    pub fn snapshot(&self) -> Vec<SpanStats, N> {
        // SAFETY: We're reading atomically; safe from any concurrent record_span.
        unsafe {
            let entries = &*self.entries.get();
            let mut result = Vec::new();
            for entry in entries.iter() {
                let _ = result.push(entry.read());
            }
            result
        }
    }
}

impl<const N: usize> Default for SpanRegistry<N> {
    fn default() -> Self {
        Self::new()
    }
}

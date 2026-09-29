use crate::types::{SpanEntry, SpanStats};
use core::sync::atomic::Ordering;

impl SpanEntry {
    pub(crate) fn new(name: &'static str) -> Self {
        Self {
            name,
            count: core::sync::atomic::AtomicUsize::new(0),
            min_micros: core::sync::atomic::AtomicUsize::new(usize::MAX),
            max_micros: core::sync::atomic::AtomicUsize::new(0),
            sum_micros: core::sync::atomic::AtomicUsize::new(0),
        }
    }

    pub(crate) fn record(&self, micros: u64) {
        let micros_usize = micros as usize;
        self.count.fetch_add(1, Ordering::Relaxed);
        self.sum_micros.fetch_add(micros_usize, Ordering::Relaxed);

        // Update min
        loop {
            let current_min = self.min_micros.load(Ordering::Acquire);
            if micros_usize >= current_min {
                break;
            }
            if self
                .min_micros
                .compare_exchange(
                    current_min,
                    micros_usize,
                    Ordering::Release,
                    Ordering::Relaxed,
                )
                .is_ok()
            {
                break;
            }
        }

        // Update max
        loop {
            let current_max = self.max_micros.load(Ordering::Acquire);
            if micros_usize <= current_max {
                break;
            }
            if self
                .max_micros
                .compare_exchange(
                    current_max,
                    micros_usize,
                    Ordering::Release,
                    Ordering::Relaxed,
                )
                .is_ok()
            {
                break;
            }
        }
    }

    pub(crate) fn read(&self) -> SpanStats {
        SpanStats {
            name: self.name,
            count: self.count.load(Ordering::Acquire) as u32,
            min_micros: {
                let m = self.min_micros.load(Ordering::Acquire);
                if m == usize::MAX {
                    0
                } else {
                    m as u64
                }
            },
            max_micros: self.max_micros.load(Ordering::Acquire) as u64,
            sum_micros: self.sum_micros.load(Ordering::Acquire) as u64,
        }
    }
}

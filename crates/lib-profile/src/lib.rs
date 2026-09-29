#![no_std]

//! Lightweight `no_std` on-target profiler for embedded systems.
//!
//! Provides a hotpath-like API (a `measure_block!` macro + per-named-span aggregation)
//! with a pluggable backend via trait seams, suitable for MCU targets without std or alloc.
//!
//! The profiler is **feature-gated**: when the `profile` feature is OFF, all instrumentation
//! compiles to zero cost (macros expand to just the inner block). When ON, spans are recorded
//! in a fixed-capacity heapless registry and emitted via a pluggable `ProfileSink`.
//!
//! # Architecture
//!
//! - `ProfileClock` trait: abstract over clock implementations (e.g., embassy-time).
//! - `ProfileSink` trait: abstract over output (e.g., defmt, RTT, memory buffer).
//! - `SpanRegistry`: fixed-capacity heapless store of named spans (count, min, max, sum µs).
//! - `measure_block!` macro: bracket a block with timing, record the delta.
//! - `report()` function: emit the registry's summary via the sink (on demand / periodic).
//!
//! # Example
//!
//! ```ignore
//! // In your platform impl:
//! struct MyClockImpl;
//! impl ProfileClock for MyClockImpl {
//!     fn now_micros(&self) -> u64 { /* embassy-time, std::time, etc. */ }
//! }
//!
//! struct MyLogSink;
//! impl ProfileSink for MyLogSink {
//!     fn log_line(&self, line: &str) { defmt::println!("{}", line); }
//! }
//!
//! // In your app:
//! #[cfg(feature = "profile")]
//! let registry = SpanRegistry::new();
//! #[cfg(feature = "profile")]
//! measure_block!(registry, "my_function", &clock, {
//!     // your code
//! });
//! #[cfg(feature = "profile")]
//! report(&registry, &MyLogSink);
//! ```

// NOTE: this crate deliberately does NOT `extern crate alloc;`. It is heapless-only (see the
// module doc above: "without std or alloc") — every container here is a `heapless::Vec`/`String`,
// so the declaration was dead. It was also actively harmful to the `no_std` MCU consumers
// (`crates/bin/mcu-rp2350/ns`): naming `alloc` puts it in this crate's transitive dep set, and any
// consumer whose compilation makes rustc load this crate's metadata then trips
// `needs_allocator` -> "no global memory allocator found but one is required" — a phantom error
// with no span that masks the real one. In particular rustc's E0432 "a similar name exists"
// suggestion pass force-loads EVERY `--extern` crate, so a single unresolved import anywhere in
// `ns` produced that allocator error even though nothing in `ns` allocates. Do not re-add it
// without a real `alloc::` use behind a feature gate.

#[cfg(feature = "profile")]
mod impl_measure_guard;
mod impl_span_entry;
mod impl_span_registry;
mod impl_span_stats;
mod types;

#[cfg(feature = "profile")]
pub use types::MeasureGuard;
pub use types::{ProfileClock, ProfileSink, SpanRegistry, SpanStats};

/// Measure execution time of a code block.
///
/// This macro mirrors hotpath's `measure_block!` ergonomics but with a pluggable backend.
/// When the `profile` feature is enabled, it brackets the block with timing and records
/// the delta to the registry. When disabled, it expands to just the block (zero cost).
///
/// # Arguments
///
/// * `$registry` - A `SpanRegistry<N>` reference
/// * `$name` - A static string label for this span
/// * `$clock` - A `&dyn ProfileClock` reference
/// * `$expr` - The code block to measure
///
/// # Example
///
/// ```ignore
/// let registry = SpanRegistry::<64>::new();
/// let clock = MyClock;
/// measure_block!(registry, "my_span", &clock, {
///     // code here
/// });
/// ```
#[macro_export]
macro_rules! measure_block {
    ($registry:expr, $name:expr, $clock:expr, $expr:expr) => {
        #[cfg(feature = "profile")]
        {
            let _guard = {
                let now = $clock.now_micros();
                $crate::MeasureGuard {
                    registry: &$registry,
                    name: $name,
                    clock: $clock,
                    start_micros: now,
                }
            };
            $expr
        }
        #[cfg(not(feature = "profile"))]
        {
            $expr
        }
    };
}

/// Emit a profiler report via the sink.
///
/// Reads a snapshot of all recorded spans and formats them as a table
/// (name, count, min, mean, max in µs).
pub fn report<const N: usize>(registry: &SpanRegistry<N>, sink: &dyn ProfileSink) {
    let snapshot = registry.snapshot();
    if snapshot.is_empty() {
        return;
    }

    sink.log_line("=== Profiler Report ===");
    sink.log_line("name                           count       min       mean        max");
    for stat in snapshot.iter() {
        // Format: name (30 chars), count (8 chars), min/mean/max (10 chars each)
        // Simple fixed-width format; overlong names are truncated.
        let name_str = if stat.name.len() > 30 {
            &stat.name[..30]
        } else {
            stat.name
        };
        // Use a core::fmt workaround: manually format into a heapless string.
        // Since we don't have alloc, we'll use a fixed-width format string approach.
        // For simplicity, log each stat separately (no alloc).
        use core::fmt::Write as FmtWrite;
        let mut buf = heapless::String::<128>::new();
        let mean = stat.mean_micros();
        let _ = write!(
            buf,
            "{:30} {:8} {:10} {:10} {:10}",
            name_str, stat.count, stat.min_micros, mean, stat.max_micros
        );
        sink.log_line(&buf);
    }
    sink.log_line("=== End Report ===");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "profile")]
    struct TestClock {
        now: core::cell::RefCell<u64>,
    }

    #[cfg(feature = "profile")]
    impl TestClock {
        fn new() -> Self {
            Self {
                now: core::cell::RefCell::new(0),
            }
        }

        fn advance(&self, micros: u64) {
            *self.now.borrow_mut() += micros;
        }
    }

    #[cfg(feature = "profile")]
    impl ProfileClock for TestClock {
        fn now_micros(&self) -> u64 {
            *self.now.borrow()
        }
    }

    struct TestSink {
        lines: core::cell::RefCell<heapless::Vec<heapless::String<128>, 32>>,
    }

    impl TestSink {
        fn new() -> Self {
            Self {
                lines: core::cell::RefCell::new(heapless::Vec::new()),
            }
        }
    }

    impl ProfileSink for TestSink {
        fn log_line(&self, line: &str) {
            let mut lines = self.lines.borrow_mut();
            let mut s = heapless::String::new();
            if s.push_str(line).is_ok() {
                let _ = lines.push(s);
            }
        }
    }

    #[test]
    fn test_span_stats_mean() {
        let stat = SpanStats {
            name: "test",
            count: 10,
            min_micros: 1,
            max_micros: 100,
            sum_micros: 550,
        };
        assert_eq!(stat.mean_micros(), 55);
    }

    #[test]
    fn test_span_stats_mean_zero_count() {
        let stat = SpanStats {
            name: "test",
            count: 0,
            min_micros: 0,
            max_micros: 0,
            sum_micros: 0,
        };
        assert_eq!(stat.mean_micros(), 0);
    }

    #[test]
    fn test_registry_record_and_read() {
        let registry = SpanRegistry::<8>::new();
        registry.record_span("test", 10);
        registry.record_span("test", 20);
        registry.record_span("test", 15);

        let snapshot = registry.snapshot();
        assert_eq!(snapshot.len(), 1);
        let stat = snapshot[0];
        assert_eq!(stat.name, "test");
        assert_eq!(stat.count, 3);
        assert_eq!(stat.min_micros, 10);
        assert_eq!(stat.max_micros, 20);
        assert_eq!(stat.sum_micros, 45);
        assert_eq!(stat.mean_micros(), 15);
    }

    #[test]
    fn test_registry_multiple_spans() {
        let registry = SpanRegistry::<8>::new();
        registry.record_span("span1", 100);
        registry.record_span("span1", 200);
        registry.record_span("span2", 50);

        let snapshot = registry.snapshot();
        assert_eq!(snapshot.len(), 2);

        let span1 = snapshot.iter().find(|s| s.name == "span1").unwrap();
        assert_eq!(span1.count, 2);
        assert_eq!(span1.sum_micros, 300);

        let span2 = snapshot.iter().find(|s| s.name == "span2").unwrap();
        assert_eq!(span2.count, 1);
        assert_eq!(span2.sum_micros, 50);
    }

    #[test]
    #[cfg(feature = "profile")]
    fn test_measure_block_with_controllable_clock() {
        let registry = SpanRegistry::<8>::new();
        let clock = TestClock::new();

        // First measurement: simulate 100 microseconds
        measure_block!(registry, "test_span", &clock, {
            clock.advance(100);
        });

        // Second measurement: simulate 150 microseconds
        measure_block!(registry, "test_span", &clock, {
            clock.advance(150);
        });

        // Third measurement: simulate 50 microseconds
        measure_block!(registry, "test_span", &clock, {
            clock.advance(50);
        });

        let snapshot = registry.snapshot();
        assert_eq!(snapshot.len(), 1);
        let stat = snapshot[0];
        assert_eq!(stat.name, "test_span");
        assert_eq!(stat.count, 3);
        assert_eq!(stat.min_micros, 50);
        assert_eq!(stat.max_micros, 150);
        assert_eq!(stat.sum_micros, 300);
        assert_eq!(stat.mean_micros(), 100);
    }

    #[test]
    fn test_report_empty() {
        let registry = SpanRegistry::<8>::new();
        let sink = TestSink::new();
        report(&registry, &sink);
        // Empty registry should not emit a report.
        assert!(sink.lines.borrow().is_empty());
    }

    #[test]
    fn test_report_nonempty() {
        let registry = SpanRegistry::<8>::new();
        registry.record_span("test_span", 100);
        let sink = TestSink::new();
        report(&registry, &sink);

        let lines = sink.lines.borrow();
        assert!(!lines.is_empty());
        // Should have header, data line, footer
        assert!(lines[0].contains("Profiler Report"));
    }
}

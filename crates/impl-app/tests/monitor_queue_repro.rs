//! HOST reproduction harness for the monitor-log-queue cross-thread corruption bug (see
//! `bin_zephyr_platform`'s `MonitorLogger`/`MONITOR_QUEUE` doc comments in `bin/zephyr/platform/src/lib.rs`).
//!
//! That crate is FFI over the Zephyr shim (host test binaries cannot link its `sz_*` symbols), so it
//! can't be exercised directly on the host. This test is instead a **faithful port** of the exact
//! concurrency shape it uses for the monitor queue:
//!   - `heapless::spsc::Queue<T, N>` — a queue documented and implemented as lock-free SPSC (see
//!     `heapless::spsc`'s module doc and `QueueInner::inner_enqueue`/`inner_dequeue`: `enqueue`
//!     does a `Relaxed` load of `tail`, writes the buffer slot, then a `Release` store of `tail` —
//!     a load-then-later-store split, not a single atomic RMW).
//!   - The `Producer` half wrapped in an `UnsafeCell`, guarded by an `AtomicBool` spinlock
//!     (`with_producer`), so TWO producer threads can share it — this mirrors
//!     `MonitorLogger::producer`/`MonitorLogger::with_producer` line for line.
//!   - ONE consumer thread draining with the raw `Consumer` half, unguarded — mirrors
//!     `rust_broker_main`'s `monitor_rx` (the sole declared consumer), running concurrently with
//!     both producers (not joined-after), matching the real firmware shape.
//!
//! This crate as a library is `no_std` outside the `host` feature, but a `tests/*.rs` integration
//! test is always its own `std`-linked binary, so this file needs neither `--features host` nor
//! any of that feature's heavy transitive deps (tokio/rumqttd/ring) — it depends only on
//! `heapless` (an unconditional, non-optional dependency of `impl-app`) plus `std::thread`. That
//! keeps it tractable to run under Miri, whose data-race detector is what actually proves (or
//! exonerates) the "spinlock around one producer handle is enough" claim in `MonitorLogger`'s doc
//! comment — no guessing from a QEMU fault dump required.
#![allow(clippy::unwrap_used)]

use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::thread;

/// Small line payload — mirrors `QueuedLogLine` in shape (a fixed-size record), just without
/// pulling in the `MONITOR_LINE_CAP`-sized `heapless::String` (irrelevant to the race; a smaller
/// payload lets more interleavings fit in a Miri-tractable run).
#[derive(Clone, Debug, PartialEq, Eq)]
struct Line {
    /// Which producer thread wrote this (0 or 1) — lets the consumer detect a corrupted/torn read
    /// that produces an impossible tag.
    producer: u8,
    /// Monotonic-per-producer sequence number.
    seq: u32,
}

const QUEUE_DEPTH: usize = 4; // small on purpose: forces frequent wraparound/full-queue races.

/// Faithful port of `MonitorLogger`: wraps the `Producer` half behind a spinlock so multiple
/// threads can call `enqueue` through it, exactly like `MonitorLogger::with_producer`.
struct SpinlockedProducer<'q> {
    producer: UnsafeCell<heapless::spsc::Producer<'q, Line>>,
    lock: AtomicBool,
    dropped: AtomicU32,
}
// SAFETY (test-only mirror of `MonitorLogger`'s own `unsafe impl Sync` + doc comment): every
// access to `producer` goes through `with_producer`'s spinlock critical section.
unsafe impl Sync for SpinlockedProducer<'_> {}

impl<'q> SpinlockedProducer<'q> {
    fn new(producer: heapless::spsc::Producer<'q, Line>) -> Self {
        SpinlockedProducer {
            producer: UnsafeCell::new(producer),
            lock: AtomicBool::new(false),
            dropped: AtomicU32::new(0),
        }
    }

    /// Identical critical-section shape to `MonitorLogger::with_producer`.
    fn with_producer<R>(&self, f: impl FnOnce(&mut heapless::spsc::Producer<'q, Line>) -> R) -> R {
        while self
            .lock
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            std::hint::spin_loop();
        }
        // SAFETY (mirrors MonitorLogger::with_producer's safety comment): the spinlock above is
        // *claimed* to guarantee exclusive access to `producer` for the duration of `f`. This test
        // exists to check whether that claim actually holds against the concurrent, unguarded
        // consumer thread.
        let producer = unsafe { &mut *self.producer.get() };
        let result = f(producer);
        self.lock.store(false, Ordering::Release);
        result
    }

    fn enqueue(&self, line: Line) {
        let dropped = self.with_producer(|p| p.enqueue(line).is_err());
        if dropped {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// Drive two producer threads (spinlock-serialized, mirroring the poll-loop thread and the broker
/// thread both calling `log::info!`/`warn!`) and one consumer thread (mirroring the sole
/// `rust_broker_main` drain loop) concurrently, then verify every dequeued item is byte-for-byte
/// intact (no torn/garbled `Line`, no impossible producer tag, no fabricated/duplicated data).
///
/// Returns the list of corruption symptoms observed (empty = clean run).
fn run_repro(iters: u32) -> Vec<String> {
    let mut problems = Vec::new();

    // Leaked to 'static for the test's duration and split exactly once here, before any
    // producer/consumer thread starts — the same single-init-before-multithreading discipline
    // `monitor_producer`/`monitor_consumer`'s safety notes require.
    let queue: &'static mut heapless::spsc::Queue<Line, QUEUE_DEPTH> =
        Box::leak(Box::new(heapless::spsc::Queue::new()));
    let (producer, mut consumer) = queue.split();

    let spin_producer = Arc::new(SpinlockedProducer::new(producer));
    // Reaches 2 once both producer threads have finished; the consumer uses this (instead of a
    // fixed spin deadline) to know it can stop draining.
    let producers_done = Arc::new(AtomicU32::new(0));

    let p0 = {
        let sp = Arc::clone(&spin_producer);
        let done = Arc::clone(&producers_done);
        thread::spawn(move || {
            for seq in 0..iters {
                sp.enqueue(Line { producer: 0, seq });
            }
            done.fetch_add(1, Ordering::Release);
        })
    };
    let p1 = {
        let sp = Arc::clone(&spin_producer);
        let done = Arc::clone(&producers_done);
        thread::spawn(move || {
            for seq in 0..iters {
                sp.enqueue(Line { producer: 1, seq });
            }
            done.fetch_add(1, Ordering::Release);
        })
    };

    let mut seen: Vec<Line> = Vec::new();
    let mut last_seq_per_producer = [None::<u32>, None::<u32>];
    let total_expected_upper_bound = iters as usize * 2;
    // Drain concurrently with the producers, terminating as soon as BOTH producers have finished
    // AND the queue is empty. (The old code spun on a fixed `iters * 10_000` deadline — ~2 billion
    // busy-spins for the 200k stress run — because dropped enqueues keep `seen.len()` below the
    // upper bound, so the `seen.len() >= upper_bound` early-exit was unreachable. That deadline
    // busy-spin, NOT a deadlock, is what ran >60s and timed tarpaulin out. Stragglers enqueued
    // after this loop breaks are still caught by the post-join drain below.)
    loop {
        if let Some(item) = consumer.dequeue() {
            if item.producer > 1 {
                problems.push(format!(
                    "impossible producer tag {} (seq={}) — corrupted queue slot",
                    item.producer, item.seq
                ));
            } else {
                let idx = item.producer as usize;
                if let Some(last) = last_seq_per_producer[idx] {
                    if item.seq <= last {
                        problems.push(format!(
                            "producer {idx} sequence went backwards/duplicated: last={last} now={}",
                            item.seq
                        ));
                    }
                }
                last_seq_per_producer[idx] = Some(item.seq);
            }
            seen.push(item);
        } else if producers_done.load(Ordering::Acquire) == 2 {
            break;
        } else {
            std::hint::spin_loop();
        }
    }

    p0.join().unwrap();
    p1.join().unwrap();
    while let Some(item) = consumer.dequeue() {
        seen.push(item);
    }

    if seen.len() > total_expected_upper_bound {
        problems.push(format!(
            "dequeued {} items but at most {total_expected_upper_bound} were ever enqueued — \
             fabricated/duplicated data",
            seen.len()
        ));
    }

    problems
}

/// Miri-sized run: small iteration count (Miri's data-race detector is what matters, not raw
/// throughput) but real `std::thread`s. Run with:
///   rtk cargo +nightly miri test -p impl-app --test monitor_queue_repro miri_sized
#[test]
fn monitor_queue_spinlocked_producer_miri_sized() {
    let iters = if cfg!(miri) { 60 } else { 400 };
    let problems = run_repro(iters);
    assert!(
        problems.is_empty(),
        "monitor queue corruption detected under concurrent producers: {problems:#?}"
    );
}

/// Native stress run (far too slow under Miri): hammer the same shape with many more iterations
/// to also catch logical corruption (lost/duplicated sequence numbers) that a short Miri run might
/// not have enough interleavings to trigger.
#[test]
#[ignore = "brute-force native stress — NOT a CI gate (slow + nondeterministic; timed tarpaulin out). \
            Concurrency correctness is proven deterministically by the *_miri_sized variants under Miri \
            (ideally Loom, per CLAUDE.md). Run on demand: cargo test -p impl-app -- --ignored"]
fn monitor_queue_spinlocked_producer_native_stress() {
    let problems = run_repro(200_000);
    assert!(
        problems.is_empty(),
        "monitor queue corruption detected under concurrent producers: {problems:#?}"
    );
}

// -------------------------------------------------------------------------------------------
// Combined-load model: SpinArena (canonical arena) + the monitor queue, driven by exactly the
// two real threads' actual workloads, concurrently — the scenario the firmware fault report
// describes ("live telemetry AND a connected client simultaneously"). `lib_store` builds
// under plain `std`/`no_std` alike (it's a pure data structure, no FFI), so it's used directly
// here rather than re-modeled; only the spinlock wrapper + the two real call sites
// (`emit_reading`-shaped write, `render_pending_topic`-shaped read) are ported, matching
// `SpinArena::with_arena` in `bin/zephyr/platform/src/lib.rs` line for line.
// -------------------------------------------------------------------------------------------
mod combined_load {
    use super::*;
    use lib_parse::canonical::CanonicalReading;
    use lib_store::CanonicalStore;

    const ARENA_DEPTH: usize = 8;

    /// Faithful port of `bin_zephyr_platform`'s `SpinArena`: one spinlock, critical section covers the
    /// whole closure, mirrors `with_arena`'s exact shape.
    struct SpinArena {
        arena: UnsafeCell<CanonicalStore<ARENA_DEPTH>>,
        lock: AtomicBool,
    }
    unsafe impl Sync for SpinArena {}

    impl SpinArena {
        fn new() -> Self {
            SpinArena {
                arena: UnsafeCell::new(CanonicalStore::new()),
                lock: AtomicBool::new(false),
            }
        }

        fn with_arena<R>(&self, f: impl FnOnce(&mut CanonicalStore<ARENA_DEPTH>) -> R) -> R {
            while self
                .lock
                .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
                .is_err()
            {
                std::hint::spin_loop();
            }
            // SAFETY: mirrors `SpinArena::with_arena` in `bin/zephyr/platform/src/lib.rs` — the spinlock above is
            // claimed to guarantee exclusive access to `arena` for the duration of `f`.
            let arena = unsafe { &mut *self.arena.get() };
            let result = f(arena);
            self.lock.store(false, Ordering::Release);
            result
        }
    }

    fn reading(total_voltage_cv: u16) -> CanonicalReading {
        CanonicalReading {
            device_address: 0,
            cell_voltages_mv: heapless::Vec::new(),
            temperatures_raw: heapless::Vec::new(),
            current_ca: -164,
            total_voltage_cv,
            remaining_capacity_cah: 0,
            typical_capacity_cah: 0,
            rated_capacity_cah: 0,
            soc_dpct: 775,
            soh_dpct: 1000,
            cycles: 2,
            port_voltage_cv: 0,
        }
    }

    /// Drive the arena + monitor queue exactly like the real firmware's two threads:
    ///   - "poll-loop" thread: `ARENA.with_arena(push_reading)` (emit_reading's Broker branch)
    ///     interleaved with monitor-queue log lines (poll loop's own `log::warn!` call sites).
    ///   - "broker" thread: `ARENA.with_arena(take_pending)` + `latest_reading().cloned()`
    ///     (render_pending_topic's read side) interleaved with its own monitor-queue log lines
    ///     AND draining the monitor queue (rust_broker_main's monitor_rx.dequeue() loop).
    ///
    /// Both threads touch the arena AND the monitor queue's producer side concurrently — the
    /// actual combined-load shape, not either mechanism in isolation.
    fn run_combined(iters: u32) -> Vec<String> {
        let mut problems = Vec::new();

        let arena = Arc::new(SpinArena::new());
        let queue: &'static mut heapless::spsc::Queue<Line, QUEUE_DEPTH> =
            Box::leak(Box::new(heapless::spsc::Queue::new()));
        let (producer, mut consumer) = queue.split();
        let spin_producer = Arc::new(SpinlockedProducer::new(producer));

        let poll_thread = {
            let arena = Arc::clone(&arena);
            let sp = Arc::clone(&spin_producer);
            thread::spawn(move || {
                for seq in 0..iters {
                    arena.with_arena(|a| a.push_reading(reading((100 + seq % 50) as u16)));
                    sp.enqueue(Line { producer: 0, seq });
                }
            })
        };
        let broker_thread = {
            let arena = Arc::clone(&arena);
            let sp = Arc::clone(&spin_producer);
            thread::spawn(move || {
                let torn_reads = 0u32;
                for seq in 0..iters {
                    // render_pending_topic's read side: clone the latest reading out while
                    // holding the lock, exactly like the real render closure does.
                    let _ = arena.with_arena(|a| a.latest_reading().cloned());
                    let _ = arena.with_arena(CanonicalStore::take_pending);
                    sp.enqueue(Line { producer: 1, seq });
                    // rust_broker_main's own monitor_rx drain — runs on the SAME thread as the
                    // arena reads above and the producer-side enqueue, matching the real
                    // broker-pump loop's single-threaded structure.
                    while consumer.dequeue().is_some() {}
                }
                torn_reads
            })
        };

        poll_thread.join().unwrap();
        let torn = broker_thread.join().unwrap();
        if torn > 0 {
            problems.push(format!("{torn} torn/inconsistent arena reads observed"));
        }
        problems
    }

    #[test]
    fn combined_arena_and_monitor_queue_miri_sized() {
        let iters = if cfg!(miri) { 40 } else { 300 };
        let problems = run_combined(iters);
        assert!(
            problems.is_empty(),
            "combined-load corruption detected: {problems:#?}"
        );
    }

    #[test]
    #[ignore = "brute-force native stress — NOT a CI gate; the *_miri_sized variant under Miri is the \
                deterministic proof (ideally Loom, per CLAUDE.md). Run on demand: cargo test -p impl-app -- --ignored"]
    fn combined_arena_and_monitor_queue_native_stress() {
        let problems = run_combined(100_000);
        assert!(
            problems.is_empty(),
            "combined-load corruption detected: {problems:#?}"
        );
    }
}

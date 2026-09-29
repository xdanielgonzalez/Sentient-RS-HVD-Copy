//! Bounded in-memory telemetry store — the "store canonical, render on output" memory model.
//!
//! Holds compact, fixed-point `CanonicalReading`/`CanonicalFault` values in a fixed-capacity,
//! overwrite-oldest ring buffer ([`CanonicalStore`]) and tracks which TMS topics have new data
//! ([`PendingTopic`]). The point is memory efficiency: the poll loop decodes telemetry straight
//! into canonical form (no serialization, no per-reading heap) and hands it here; a separate
//! consumer renders whichever topics are pending, one at a time, straight onto the wire — so the
//! serialized (~10x inflated) form is never held in memory. Motivated by the MCU stack budget (the
//! poll loop cannot afford to build+serialize TMS structs in its own frame), but the pattern is
//! target-agnostic: the host path may adopt it for the same efficiency.
//!
//! Pure `no_std`/no-alloc data structure — no `static`, no locking, no I/O. The caller wires an
//! instance into `static` storage behind a lock where cross-thread sharing is needed (e.g. the MCU
//! broker pump's spinlock-guarded static); this crate only has to be correct and host-testable in
//! isolation. See ADR 0009 for why this is its own crate — both the app policy and every platform
//! adapter depend on it, none on each other.
//!
//! # Example
//!
//! One full cycle on a depth-4 store — the poll-loop side writes, the broker-pump side drains:
//!
//! ```
//! use lib_store::{CanonicalFault, CanonicalReading, CanonicalStore, PendingTopic};
//!
//! let mut store: CanonicalStore<4> = CanonicalStore::new();
//!
//! // Poll side: decode straight into compact fixed-point canonical form and push — no
//! // serialization, no per-reading heap.
//! store.push_reading(CanonicalReading {
//!     device_address: 0x00,
//!     cell_voltages_mv: heapless::Vec::from_slice(&[3_321, 3_320, 3_322, 3_319])?,
//!     current_ca: -164,        // -1.64 A (discharging)
//!     total_voltage_cv: 5_312, // 53.12 V
//!     soc_dpct: 775,           // 77.5 %
//! #   temperatures_raw: heapless::Vec::from_slice(&[2_981, 2_979])?,
//! #   remaining_capacity_cah: 21_700,
//! #   typical_capacity_cah: 28_000,
//! #   rated_capacity_cah: 28_000,
//! #   soh_dpct: 1_000,
//! #   cycles: 2,
//! #   port_voltage_cv: 5_310,
//!     // …remaining fixed-point fields…
//! });
//! store.set_fault(CanonicalFault {
//!     device_address: 0x00,
//!     low_capacity_warning: true,
//!     ..Default::default()
//! });
//!
//! // Pump side: drain what's new since the last cycle, render each topic onto the wire.
//! let pending = store.take_pending();
//! assert_eq!(pending.len(), 3);
//! assert!(pending.contains(&PendingTopic::DcMeasurementUpdate));
//! assert!(pending.contains(&PendingTopic::PowerHardwareUpdate));
//! assert!(pending.contains(&PendingTopic::ActiveDiagnosticState));
//!
//! assert_eq!(store.latest_reading().map(|r| r.total_voltage_cv), Some(5_312));
//! assert!(store.take_pending().is_empty()); // drained — nothing new until the next write
//! # Ok::<(), heapless::CapacityError>(())
//! ```

#![no_std]
#![forbid(unsafe_code)]

mod prelude;
mod types;

mod impl_canonical_store;

pub use types::*;

#[cfg(test)]
mod tests {
    use super::*;

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

    fn fault_with(high_voltage: bool) -> CanonicalFault {
        CanonicalFault {
            device_address: 0,
            high_voltage_protection: high_voltage,
            ..Default::default()
        }
    }

    #[test]
    fn empty_store_has_no_latest() {
        let store: CanonicalStore<4> = CanonicalStore::new();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);
        assert!(store.latest_reading().is_none());
        assert!(store.latest_fault().is_none());
    }

    #[test]
    fn push_then_latest_returns_most_recent() {
        let mut store: CanonicalStore<4> = CanonicalStore::new();
        store.push_reading(reading(100));
        store.push_reading(reading(200));
        assert_eq!(store.len(), 2);
        assert_eq!(
            store.latest_reading().map(|r| r.total_voltage_cv),
            Some(200)
        );
    }

    #[test]
    fn wraparound_overwrites_oldest_and_keeps_latest() {
        let mut store: CanonicalStore<3> = CanonicalStore::new();
        for v in 1..=5u16 {
            store.push_reading(reading(v));
        }
        // Depth 3: only the last 3 pushes (3, 4, 5) survive.
        assert_eq!(store.len(), 3);
        assert_eq!(store.latest_reading().map(|r| r.total_voltage_cv), Some(5));
    }

    #[test]
    fn set_fault_then_latest_fault_returns_it() {
        let mut store: CanonicalStore<4> = CanonicalStore::new();
        assert!(store.latest_fault().is_none());
        store.set_fault(fault_with(true));
        assert_eq!(
            store.latest_fault().map(|f| f.high_voltage_protection),
            Some(true)
        );
        store.set_fault(fault_with(false));
        assert_eq!(
            store.latest_fault().map(|f| f.high_voltage_protection),
            Some(false)
        );
    }

    #[test]
    fn push_reading_marks_both_telemetry_topics_pending() {
        let mut store: CanonicalStore<4> = CanonicalStore::new();
        store.push_reading(reading(100));
        let pending = store.take_pending();
        assert_eq!(pending.len(), 2);
        assert!(pending.contains(&PendingTopic::DcMeasurementUpdate));
        assert!(pending.contains(&PendingTopic::PowerHardwareUpdate));
        assert!(!pending.contains(&PendingTopic::ActiveDiagnosticState));
    }

    #[test]
    fn set_fault_marks_only_diagnostic_topic_pending() {
        let mut store: CanonicalStore<4> = CanonicalStore::new();
        store.set_fault(fault_with(true));
        let pending = store.take_pending();
        assert_eq!(pending.len(), 1);
        assert!(pending.contains(&PendingTopic::ActiveDiagnosticState));
    }

    #[test]
    fn take_pending_drains_and_resets() {
        let mut store: CanonicalStore<4> = CanonicalStore::new();
        store.push_reading(reading(100));
        store.set_fault(fault_with(true));
        let first = store.take_pending();
        assert_eq!(first.len(), 3);

        // Drained: nothing pending until the next write.
        let second = store.take_pending();
        assert!(second.is_empty());
    }

    #[test]
    fn repeated_pushes_without_drain_do_not_duplicate_pending_entries() {
        let mut store: CanonicalStore<4> = CanonicalStore::new();
        store.push_reading(reading(100));
        store.push_reading(reading(200));
        store.push_reading(reading(300));
        let pending = store.take_pending();
        // Still exactly 2 entries (no duplicates), even though 3 readings were pushed.
        assert_eq!(pending.len(), 2);
    }

    #[test]
    fn default_matches_new() {
        let a: CanonicalStore<4> = CanonicalStore::new();
        let b: CanonicalStore<4> = CanonicalStore::default();
        assert_eq!(a.len(), b.len());
        assert_eq!(a.is_empty(), b.is_empty());
    }
}

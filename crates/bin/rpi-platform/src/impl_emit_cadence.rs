use super::prelude::*;

impl Default for EmitCadence {
    fn default() -> Self {
        EmitCadence {
            rate_last: heapless::Vec::new(),
            seplos_fault: None,
            seplos_emit: None,
            inverter_fault: None,
            inverter_emit: None,
        }
    }
}

impl EmitCadence {
    /// Minimum interval between `PowerHardwareUpdate`/`StorageUpdate` publishes — spec Slow /
    /// Periodic 10 s with a "shall not send faster" rate limit (IDL 216-234; MIL-STD-3071
    /// §4.9.10.2).
    pub const PERIODIC_10S: Duration = Duration::from_secs(10);

    /// Safety-net re-publish interval for `ActiveDiagnosticState` when the fault set is unchanged.
    /// A documented PublishLast / durability emulation: the in-process broker publisher can't retain
    /// (see `bin_rpi::tasks::heartbeat`'s DeviceInfo note), so a periodic re-publish bounds how
    /// stale a late subscriber's view can be. PROVISIONAL value — comfortably above the fault-poll
    /// cadence so a steady-state device still deduplicates redundant diagnostic publishes.
    pub const ADS_SAFETY_NET: Duration = Duration::from_secs(30);

    /// Convert a monotonic TMS timestamp into a [`Duration`] since the platform origin — the poll
    /// loop's [`ClockMonotonic`] is already "seconds/nanoseconds since `HostPlatform::start`", so
    /// this is a straight reinterpretation (no wall-clock, deterministic under test).
    pub fn since_origin(now: ClockMonotonic) -> Duration {
        Duration::new(now.seconds as u64, now.nanoseconds)
    }

    /// Rate-limit gate for a periodic topic: returns `true` (and records `now` as this topic's new
    /// last-publish time) when at least `min_interval` has elapsed since it last published, or when
    /// it has never published (the at-start emit). Returns `false` — suppress this publish — when
    /// it published too recently. Only the two 10 s topics are ever passed here, so the bounded
    /// `rate_last` (capacity 2) never overflows.
    pub fn rate_allow(
        &mut self,
        topic: &'static str,
        min_interval: Duration,
        now: Duration,
    ) -> bool {
        for entry in self.rate_last.iter_mut() {
            if entry.0 == topic {
                if now.saturating_sub(entry.1) >= min_interval {
                    entry.1 = now;
                    return true;
                }
                return false;
            }
        }
        // First publish for this topic: always allowed (at-start), record it.
        let _ = self.rate_last.push((topic, now));
        true
    }

    /// `ActiveDiagnosticState` publish gate for the Seplos path: publish when the fault set changed
    /// (On Change), or the safety-net interval elapsed (durability emulation), or it has never
    /// published (At Start). Records the new snapshot + time on any publish.
    pub fn seplos_ads_due(&mut self, fault: &CanonicalFault, now: Duration) -> bool {
        let changed = self.seplos_fault.as_ref() != Some(fault);
        let stale = match self.seplos_emit {
            Some(t) => now.saturating_sub(t) >= Self::ADS_SAFETY_NET,
            None => true,
        };
        if changed || stale {
            self.seplos_fault = Some(*fault);
            self.seplos_emit = Some(now);
            true
        } else {
            false
        }
    }

    /// `ActiveDiagnosticState` publish gate for the inverter path — same On-Change + safety-net
    /// semantics as [`EmitCadence::seplos_ads_due`], but the change key is `(fault_set, mode)`: a
    /// QMOD mode transition is an on-change event because the mode-fault DTC is folded into the same
    /// diagnostic payload (D10).
    pub fn inverter_ads_due(
        &mut self,
        fault: &CanonicalInverterFault,
        mode: InverterMode,
        now: Duration,
    ) -> bool {
        let key = (*fault, mode);
        let changed = self.inverter_fault != Some(key);
        let stale = match self.inverter_emit {
            Some(t) => now.saturating_sub(t) >= Self::ADS_SAFETY_NET,
            None => true,
        };
        if changed || stale {
            self.inverter_fault = Some(key);
            self.inverter_emit = Some(now);
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::EmitCadence;
    use lib_mapping::schema::ClockMonotonic;
    use lib_parse::canonical::{CanonicalFault, CanonicalInverterFault, InverterMode};
    use std::time::Duration;

    fn at(secs: u64) -> Duration {
        Duration::from_secs(secs)
    }

    #[test]
    fn rate_allow_first_emit_then_gates_until_interval() {
        let mut c = EmitCadence::default();
        // At-start publish is always allowed.
        assert!(c.rate_allow("PowerHardwareUpdate", EmitCadence::PERIODIC_10S, at(0)));
        // Within 10 s: suppressed.
        assert!(!c.rate_allow("PowerHardwareUpdate", EmitCadence::PERIODIC_10S, at(2)));
        assert!(!c.rate_allow("PowerHardwareUpdate", EmitCadence::PERIODIC_10S, at(9)));
        // At/after 10 s: allowed again, and the window restarts from there.
        assert!(c.rate_allow("PowerHardwareUpdate", EmitCadence::PERIODIC_10S, at(10)));
        assert!(!c.rate_allow("PowerHardwareUpdate", EmitCadence::PERIODIC_10S, at(15)));
        assert!(c.rate_allow("PowerHardwareUpdate", EmitCadence::PERIODIC_10S, at(20)));
    }

    #[test]
    fn rate_allow_tracks_topics_independently() {
        let mut c = EmitCadence::default();
        assert!(c.rate_allow("PowerHardwareUpdate", EmitCadence::PERIODIC_10S, at(0)));
        // A different topic has its own independent window.
        assert!(c.rate_allow("StorageUpdate", EmitCadence::PERIODIC_10S, at(0)));
        assert!(!c.rate_allow("StorageUpdate", EmitCadence::PERIODIC_10S, at(3)));
    }

    #[test]
    fn since_origin_reinterprets_clock() {
        let d = EmitCadence::since_origin(ClockMonotonic {
            seconds: 12,
            nanoseconds: 500_000_000,
        });
        assert_eq!(d, Duration::new(12, 500_000_000));
    }

    #[test]
    fn seplos_ads_emits_at_start_and_on_change_only() {
        let mut c = EmitCadence::default();
        let clear = CanonicalFault::default();
        let faulted = CanonicalFault {
            high_voltage_protection: true,
            ..CanonicalFault::default()
        };

        // At-start: publish.
        assert!(c.seplos_ads_due(&clear, at(0)));
        // Unchanged, within safety net: suppress.
        assert!(!c.seplos_ads_due(&clear, at(10)));
        // Changed set: publish.
        assert!(c.seplos_ads_due(&faulted, at(11)));
        // Same faulted set again shortly after: suppress.
        assert!(!c.seplos_ads_due(&faulted, at(12)));
        // Unchanged but past the safety net (30 s since last emit at 11): publish.
        assert!(c.seplos_ads_due(&faulted, at(41)));
    }

    #[test]
    fn inverter_ads_treats_mode_change_as_on_change() {
        let mut c = EmitCadence::default();
        let fault = CanonicalInverterFault::default();

        assert!(c.inverter_ads_due(&fault, InverterMode::Line, at(0)));
        // Same fault + same mode: suppress.
        assert!(!c.inverter_ads_due(&fault, InverterMode::Line, at(2)));
        // Same fault, mode transitioned to Fault: on-change publish (mode-fault DTC folds in).
        assert!(c.inverter_ads_due(&fault, InverterMode::Fault, at(3)));
    }
}

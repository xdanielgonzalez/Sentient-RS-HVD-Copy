use crate::{Health, UNHEALTHY_AFTER_CYCLES};

impl Health {
    #[inline]
    fn bump(counter: &mut u32) {
        *counter = counter.saturating_add(1);
    }

    /// Record a telemetry outcome.
    pub fn record_telemetry(&mut self, ok: bool) {
        Self::bump(if ok {
            &mut self.telemetry_ok
        } else {
            &mut self.telemetry_err
        });
    }
    /// Record a fault outcome.
    pub fn record_fault(&mut self, ok: bool) {
        Self::bump(if ok {
            &mut self.fault_ok
        } else {
            &mut self.fault_err
        });
    }
    /// Record a publish outcome.
    pub fn record_publish(&mut self, ok: bool) {
        Self::bump(if ok {
            &mut self.publish_ok
        } else {
            &mut self.publish_err
        });
    }
    /// Close out a cycle: bump the cycle count and update the consecutive-failure streak.
    pub fn end_cycle(&mut self, any_success: bool) {
        Self::bump(&mut self.cycles);
        if any_success {
            self.consecutive_failed_cycles = 0;
        } else {
            Self::bump(&mut self.consecutive_failed_cycles);
        }
    }
    /// Healthy while fewer than [`UNHEALTHY_AFTER_CYCLES`] cycles have failed back to back.
    pub fn healthy(&self) -> bool {
        self.consecutive_failed_cycles < UNHEALTHY_AFTER_CYCLES
    }
}

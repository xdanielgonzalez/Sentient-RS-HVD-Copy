use crate::preflight::{Outcome, Report};

impl Report {
    /// An empty report.
    pub const fn new() -> Self {
        Report {
            entries: heapless::Vec::new(),
        }
    }

    /// Record one check's outcome. Silently dropped if the report is already full — that should
    /// never happen in practice ([`MAX_CHECKS`](crate::preflight::MAX_CHECKS) comfortably exceeds any real check list) and would
    /// indicate a bug, but it must never panic.
    pub fn record(&mut self, entry: (&'static str, Outcome)) {
        let _ = self.entries.push(entry);
    }

    /// Count of [`Outcome::Pass`] entries.
    pub fn passed(&self) -> usize {
        self.entries
            .iter()
            .filter(|(_, o)| matches!(o, Outcome::Pass))
            .count()
    }

    /// Count of [`Outcome::Warn`] entries.
    pub fn warned(&self) -> usize {
        self.entries
            .iter()
            .filter(|(_, o)| matches!(o, Outcome::Warn(_)))
            .count()
    }

    /// Count of [`Outcome::Fail`] entries.
    pub fn failed(&self) -> usize {
        self.entries
            .iter()
            .filter(|(_, o)| matches!(o, Outcome::Fail(_)))
            .count()
    }

    /// True iff no check failed (warnings do not block).
    pub fn ok(&self) -> bool {
        self.failed() == 0
    }

    /// True iff no check failed, **except** the named `id` — for a caller that has a documented
    /// degraded-start path for one specific check (e.g. the host platform's `serial_present`: a
    /// missing/unopenable port starts the device DEGRADED via [`crate::preflight::checks::SerialPresent`]'s
    /// companion transport fallback rather than refusing to boot — see bug #172). Every other
    /// failed check still blocks startup; this does not change [`Report::ok`] or [`Report::failed`]
    /// (which a caller still uses for logging/counters), only the narrow "may I proceed" gate.
    pub fn ok_except(&self, id: &str) -> bool {
        self.entries
            .iter()
            .all(|(check_id, o)| !matches!(o, Outcome::Fail(_)) || *check_id == id)
    }

    /// Log the aggregate result line: `log::error!` if anything failed, `log::info!` otherwise.
    pub fn finish(&self) {
        let passed = self.passed();
        let warned = self.warned();
        let failed = self.failed();
        if failed > 0 {
            log::error!("preflight complete: {passed} passed, {warned} warned, {failed} failed");
        } else {
            log::info!("preflight complete: {passed} passed, {warned} warned, {failed} failed");
        }
    }
}

impl Default for Report {
    fn default() -> Self {
        Report::new()
    }
}

/// Outcome of one poll cycle.
pub struct CycleOutcome {
    /// True if at least one exchange this cycle succeeded (drives backoff reset).
    pub any_success: bool,
}

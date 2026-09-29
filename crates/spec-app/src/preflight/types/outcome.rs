/// The result of a single check.
#[derive(Debug, Clone, Copy)]
pub enum Outcome {
    /// The check succeeded.
    Pass,
    /// The check succeeded with a caveat worth surfacing, but does not block startup.
    Warn(&'static str),
    /// The check failed.
    Fail(&'static str),
}

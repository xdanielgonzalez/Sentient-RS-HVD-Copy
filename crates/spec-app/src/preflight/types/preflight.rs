use crate::preflight::Report;

/// The DI seam: a platform builds its own [`Preflight::Ctx`] and lists its own ordered checks in
/// [`Preflight::preflight`] (typically via [`run_checks!`](crate::preflight::run_checks)).
#[allow(async_fn_in_trait)]
pub trait Preflight {
    /// The context type this platform's checks run against.
    type Ctx;
    /// Build the context passed to every check.
    fn context(&mut self) -> Self::Ctx;
    /// Run this platform's ordered check list and return the assembled report.
    async fn preflight(&mut self) -> Report;
}

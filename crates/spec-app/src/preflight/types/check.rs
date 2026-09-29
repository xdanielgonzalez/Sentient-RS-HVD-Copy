use crate::preflight::Outcome;

/// A single preflight check against context `C`. Static dispatch only: checks are concrete unit
/// structs listed explicitly per platform, never boxed or type-erased.
///
/// A platform-specific check is a unit struct implementing this trait against that platform's own
/// context type, driven through [`run_one`](crate::preflight::run_one) (or, for a whole ordered
/// list, [`run_checks!`](crate::preflight::run_checks)):
///
/// ```rust
/// use spec_app::preflight::{run_one, Check, Outcome};
///
/// # fn block_on<F: core::future::Future>(fut: F) -> F::Output {
/// #     let mut fut = core::pin::pin!(fut);
/// #     let mut cx = core::task::Context::from_waker(core::task::Waker::noop());
/// #     loop {
/// #         if let core::task::Poll::Ready(out) = fut.as_mut().poll(&mut cx) {
/// #             return out;
/// #         }
/// #     }
/// # }
/// struct Ctx {
///     dips: usize,
/// }
///
/// struct HasDips;
/// impl Check<Ctx> for HasDips {
///     fn id(&self) -> &'static str {
///         "has_dips"
///     }
///     async fn run(&self, ctx: &Ctx) -> Outcome {
///         if ctx.dips > 0 {
///             Outcome::Pass
///         } else {
///             Outcome::Fail("no DIP addresses configured")
///         }
///     }
/// }
///
/// let ctx = Ctx { dips: 2 };
/// let (id, outcome) = block_on(run_one(&HasDips, &ctx));
/// assert_eq!(id, "has_dips");
/// assert!(matches!(outcome, Outcome::Pass));
/// ```
#[allow(async_fn_in_trait)]
pub trait Check<C> {
    /// A stable, log-friendly identifier for this check.
    fn id(&self) -> &'static str;
    /// Run the check against `ctx`.
    async fn run(&self, ctx: &C) -> Outcome;
}

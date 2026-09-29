//! Preflight-check subsystem.
//!
//! A preflight check is a small, statically-dispatched [`Check`] run once at startup before the
//! main poll loop begins. Checks are concrete unit structs (no `dyn`, no `Box`, no alloc — async
//! trait objects require alloc, which is forbidden here) generic over a platform-supplied context
//! type. Each platform implements [`Preflight`] to build its own `Ctx` and list its own ordered
//! checks via [`run_checks!`]; results collect into a bounded [`Report`].
//!
//! Generic, platform-agnostic checks live in the [`checks`] module, implemented against the
//! [`capabilities`] traits a platform's `Ctx` opts into.
//!
//! Folded from the former `lib-preflight` crate (#286) into a module of `impl-app` (#286), then
//! moved again into `spec-app` (ADR 0009, WS1) as adapter-shared contract vocabulary — no_std,
//! unchanged behavior, only the crate boundary + import paths moved each time.

pub mod capabilities;
pub mod checks;

mod impl_report;
mod types;

pub use types::*;

/// Max checks a single [`Report`] can hold.
pub const MAX_CHECKS: usize = 32;

/// Run one check, logging its start and completion, and return its `(id, outcome)` entry.
pub async fn run_one<C, K: Check<C>>(check: &K, ctx: &C) -> (&'static str, Outcome) {
    log::info!("preflight: {} ...", check.id());
    let outcome = check.run(ctx).await;
    match outcome {
        Outcome::Pass => log::info!("preflight: {} = PASS", check.id()),
        Outcome::Warn(reason) => log::warn!("preflight: {} = WARN: {reason}", check.id()),
        Outcome::Fail(reason) => log::error!("preflight: {} = FAIL: {reason}", check.id()),
    }
    (check.id(), outcome)
}

/// Register and run an ordered list of checks against `ctx`, returning the assembled [`Report`].
///
/// The context opts into whichever [`capabilities`](crate::preflight::capabilities) traits its
/// listed checks need — here both generic checks
/// ([`SerialPresent`](crate::preflight::checks::SerialPresent),
/// [`ConfigValid`](crate::preflight::checks::ConfigValid)) run against one platform context:
///
/// ```rust
/// use spec_app::preflight::capabilities::{ConfigCap, SerialCap};
/// use spec_app::preflight::checks::{ConfigValid, SerialPresent};
/// use spec_app::preflight::run_checks;
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
///     port: &'static str,
/// }
/// impl SerialCap for Ctx {
///     fn serial_present(&self) -> bool {
///         false // e.g. the configured port did not enumerate
///     }
///     fn serial_label(&self) -> &str {
///         self.port
///     }
/// }
/// impl ConfigCap for Ctx {
///     fn config_check(&self) -> Result<(), &'static str> {
///         Ok(())
///     }
/// }
///
/// let ctx = Ctx { port: "/dev/ttyUSB0" };
/// let report = block_on(async { run_checks!(ctx => SerialPresent, ConfigValid) });
///
/// assert_eq!(report.passed(), 1);
/// assert_eq!(report.failed(), 1);
/// assert!(!report.ok());
/// // The host degraded-start path: only the serial check failed, so startup may proceed.
/// assert!(report.ok_except("serial_present"));
/// ```
#[macro_export]
macro_rules! run_checks {
    ($ctx:expr => $($check:expr),+ $(,)?) => {{
        let mut report = $crate::preflight::Report::new();
        $(
            report.record($crate::preflight::run_one(&$check, &$ctx).await);
        )+
        report.finish();
        report
    }};
}

// `#[macro_export]` always places the macro at the crate root (`spec_app::run_checks`); this
// re-export also makes it reachable at its lexical home (`spec_app::preflight::run_checks`), so
// callers can import the whole preflight surface from one path.
pub use crate::run_checks;

// Minimal inline block_on: no runtime dependency needed for a future that never actually yields
// (every `Check` impl in this crate's tests resolves in one poll). Built entirely from stable
// safe APIs — `core::pin::pin!` for the stack-pinned future and `Waker::noop()` (stable since
// 1.85; this workspace's MSRV is 1.91) for the no-op waker — so this helper needs no `unsafe`,
// matching the crate-wide `#![forbid(unsafe_code)]`. `pub(crate)` (rather than nested inside
// `mod tests`) so the `checks::*` submodules' own test modules can drive their `Check::run`
// futures too, without each duplicating a block_on.
#[cfg(test)]
pub(crate) fn pollster_block_on<F: core::future::Future>(fut: F) -> F::Output {
    use core::task::{Context, Poll, Waker};

    let mut fut = core::pin::pin!(fut);
    let waker = Waker::noop();
    let mut cx = Context::from_waker(waker);
    loop {
        if let Poll::Ready(out) = fut.as_mut().poll(&mut cx) {
            return out;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_counts_and_ok() {
        let mut report = Report::new();
        assert!(report.ok());
        report.record(("a", Outcome::Pass));
        report.record(("b", Outcome::Warn("careful")));
        assert_eq!(report.passed(), 1);
        assert_eq!(report.warned(), 1);
        assert_eq!(report.failed(), 0);
        assert!(report.ok());

        report.record(("c", Outcome::Fail("broken")));
        assert_eq!(report.failed(), 1);
        assert!(!report.ok());
    }

    #[test]
    fn default_is_empty_and_ok() {
        let report = Report::default();
        assert_eq!(report.passed() + report.warned() + report.failed(), 0);
        assert!(report.ok());
    }

    #[test]
    fn report_overflow_caps_without_panic() {
        // Recording more than MAX_CHECKS entries must never panic — `Report::record` silently
        // drops entries past capacity (see its doc comment).
        let mut report = Report::new();
        for i in 0..(MAX_CHECKS + 2) {
            // Leak a distinct 'static str per iteration isn't available in no_std/no_alloc test
            // code without alloc; reuse a small fixed set of ids instead — uniqueness of the id
            // doesn't matter for this test, only that record() never panics and the count caps.
            let id: &'static str = if i % 2 == 0 {
                "check_even"
            } else {
                "check_odd"
            };
            report.record((id, Outcome::Pass));
        }
        assert_eq!(
            report.passed(),
            MAX_CHECKS,
            "must cap at MAX_CHECKS, not overflow"
        );
        assert_eq!(
            report.passed() + report.warned() + report.failed(),
            MAX_CHECKS
        );
    }

    #[test]
    fn ok_except_matrix() {
        // none failed -> ok_except is true for any id, named or not.
        let mut none_failed = Report::new();
        none_failed.record(("a", Outcome::Pass));
        none_failed.record(("b", Outcome::Warn("careful")));
        assert!(none_failed.ok_except("a"));
        assert!(none_failed.ok_except("nonexistent"));
        assert!(none_failed.ok());

        // only the named check fails -> ok_except(that id) true, but ok() is still false.
        let mut only_named_fails = Report::new();
        only_named_fails.record(("a", Outcome::Pass));
        only_named_fails.record(("serial_present", Outcome::Fail("no port")));
        assert!(only_named_fails.ok_except("serial_present"));
        assert!(!only_named_fails.ok());

        // a DIFFERENT check id fails -> ok_except(the other id) is false.
        let mut different_id_fails = Report::new();
        different_id_fails.record(("config_valid", Outcome::Fail("bad config")));
        assert!(!different_id_fails.ok_except("serial_present"));
        assert!(!different_id_fails.ok());

        // both the named check AND another check fail -> ok_except is false (only the exact
        // named id is excused; every other failure still blocks).
        let mut both_fail = Report::new();
        both_fail.record(("serial_present", Outcome::Fail("no port")));
        both_fail.record(("config_valid", Outcome::Fail("bad config")));
        assert!(!both_fail.ok_except("serial_present"));
        assert!(!both_fail.ok());
    }

    struct FakeCtx;
    struct AlwaysPass;
    struct AlwaysWarn;
    struct AlwaysFail;

    impl Check<FakeCtx> for AlwaysPass {
        fn id(&self) -> &'static str {
            "always_pass"
        }
        async fn run(&self, _ctx: &FakeCtx) -> Outcome {
            Outcome::Pass
        }
    }

    impl Check<FakeCtx> for AlwaysWarn {
        fn id(&self) -> &'static str {
            "always_warn"
        }
        async fn run(&self, _ctx: &FakeCtx) -> Outcome {
            Outcome::Warn("careful")
        }
    }

    impl Check<FakeCtx> for AlwaysFail {
        fn id(&self) -> &'static str {
            "always_fail"
        }
        async fn run(&self, _ctx: &FakeCtx) -> Outcome {
            Outcome::Fail("broken")
        }
    }

    #[test]
    fn run_one_returns_id_and_outcome() {
        let outcome = pollster_block_on(run_one(&AlwaysPass, &FakeCtx));
        assert_eq!(outcome.0, "always_pass");
        assert!(matches!(outcome.1, Outcome::Pass));
    }

    #[test]
    fn run_checks_macro_collects_pass_warn_and_fail() {
        async fn go() -> Report {
            let ctx = FakeCtx;
            run_checks!(ctx => AlwaysPass, AlwaysWarn, AlwaysFail)
        }
        let report = pollster_block_on(go());
        assert_eq!(report.passed(), 1);
        assert_eq!(report.warned(), 1);
        assert_eq!(report.failed(), 1);
        assert!(!report.ok());
    }
}

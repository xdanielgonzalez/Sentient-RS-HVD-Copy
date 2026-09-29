/// `dds-probe` top-level error — either a bad CLI invocation or a `lib-dds` seam failure.
/// `Display`/`Error` impls live in `impl_app_error.rs` (CLAUDE.md type/impl split).
#[derive(Debug)]
pub(crate) enum AppError {
    Usage(String),
    Dds(lib_dds::DdsError),
}

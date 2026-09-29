//! Capability traits a platform's [`Preflight::Ctx`](crate::preflight::Preflight::Ctx) opts into,
//! so generic checks in [`crate::preflight::checks`] are reusable across platforms instead of
//! duplicated per target.

/// A context that can report on a configured serial device.
pub trait SerialCap {
    /// True if the configured serial device is present and openable.
    fn serial_present(&self) -> bool;
    /// A human-readable label for the configured serial device (for logs).
    fn serial_label(&self) -> &str;
}

/// A context that can validate its loaded configuration.
pub trait ConfigCap {
    /// `Ok(())` if the configuration is valid; `Err(reason)` otherwise.
    fn config_check(&self) -> Result<(), &'static str>;
}

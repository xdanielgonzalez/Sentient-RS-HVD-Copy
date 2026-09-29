/// The ModemManager cellular-GPS tier: shells out to `<binary> -m any --location-get -J` and
/// parses its JSON `modem.location.gps` sub-object (see `impl_modem_manager.rs`). An absent
/// binary is a normal per-tick tier failure (logged, not a hard error) — this is how the tier
/// self-disables on a device with no ModemManager installed.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ModemManagerProvider {
    pub binary: String,
    pub timeout_ms: u64,
}

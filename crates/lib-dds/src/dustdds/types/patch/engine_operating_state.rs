use dust_dds::infrastructure::type_support::TypeSupport;

/// dust_dds patch of `tms::EngineOperatingState` — a fieldless enum (`@extensibility(APPENDABLE) enum
/// EngineOperatingState`, idl:1811), member-only (`EngineState.operating_state`). Variant order = the
/// IDL declaration order (0-based INT32 discriminants), matching `lib_tms::generated::EngineOperatingState`
/// and the chains' `enum_u32` encoding. Like the other enum patch types (`device_role`/`config_id`/
/// `dtc_severity`) this carries only `#[dust_dds(nested)]` — the enum discriminator wire form is
/// extensibility-independent. Derive-only: no hand impl, so this is a single-type file by design.
///
/// Exclusive to the `PowerHardwareUpdate` closure (via `EngineState`); authored here by
/// wp-topic-powerhardware. Not in the original WP file list — flagged for wp-patch-mod wiring.
#[derive(TypeSupport, Clone, Copy, Debug, PartialEq, Eq)]
#[dust_dds(nested)]
pub enum EngineOperatingState {
    Unknown,
    Stopped,
    Heating,
    Starting,
    Idling,
    Warmup,
    Running,
    Cooldown,
    Stopping,
    Disabled,
}

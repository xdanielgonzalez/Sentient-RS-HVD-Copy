use dust_dds::infrastructure::type_support::TypeSupport;

/// dust_dds patch of `tms::CableSenseStatus` — a fieldless enum (`@extensibility(APPENDABLE) enum
/// CableSenseStatus` mil-std-3071_data_model.idl:432), member-only (`PowerPortStatus.cable_status`,
/// a required member). Variant order = the IDL declaration order (0-based INT32 discriminants),
/// matching `lib_tms::generated::CableSenseStatus`. Enum patch convention: `#[dust_dds(nested)]`
/// only — the enum discriminator wire form is extensibility-independent. Derive-only: no hand impl,
/// so this is a single-type file by design.
///
/// Exclusive to the `PowerPortState` closure (via `PowerPortStatus`); authored here by
/// wp-topic-small5. Not in the original WP file list — flagged for wp-patch-mod wiring.
#[derive(TypeSupport, Clone, Copy, Debug, PartialEq, Eq)]
#[dust_dds(nested)]
pub enum CableSenseStatus {
    Unknown,
    Disconnected,
    Connected,
}

use dust_dds::infrastructure::type_support::TypeSupport;

/// dust_dds patch of `tms::PowerPortDirectionality` — a fieldless enum, member-only
/// (`PowerPortInfo.directionality`). Variant order = the IDL declaration order (0-based INT32
/// discriminants), matching `lib_tms::generated::PowerPortDirectionality` (note the `None` variant
/// is the IDL `PPD_NONE`, not `Option::None`). Enum patch convention: `#[dust_dds(nested)]` only.
/// Derive-only: no hand impl, so this is a single-type file by design.
#[derive(TypeSupport, Clone, Copy, Debug, PartialEq, Eq)]
#[dust_dds(nested)]
pub enum PowerPortDirectionality {
    Unknown,
    None,
    In,
    Out,
    InOut,
}

use dust_dds::infrastructure::type_support::TypeSupport;

/// dust_dds patch of `tms::GroundingDesignType` — a fieldless enum, member-only
/// (`GroundingInfo.ground_type`). Variant order = the IDL declaration order (0-based INT32
/// discriminants), matching `lib_tms::generated::GroundingDesignType`. Enum patch convention:
/// `#[dust_dds(nested)]` only. Derive-only: no hand impl, so this is a single-type file by design.
#[derive(TypeSupport, Clone, Copy, Debug, PartialEq, Eq)]
#[dust_dds(nested)]
pub enum GroundingDesignType {
    Unknown,
    Ungrounded,
    Solid,
    HighResistance,
    LowResistance,
    Reactance,
}

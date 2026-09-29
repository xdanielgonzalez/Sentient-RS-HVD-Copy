use dust_dds::infrastructure::type_support::TypeSupport;

/// dust_dds patch of `tms::PowerConnectorType` — a fieldless enum, member-only
/// (`PowerConnectorInfo.physical_type`). Variant order = the IDL declaration order (0-based INT32
/// discriminants), matching `lib_tms::generated::PowerConnectorType`. Enum patch convention:
/// `#[dust_dds(nested)]` only. Derive-only: no hand impl, so this is a single-type file by design.
#[derive(TypeSupport, Clone, Copy, Debug, PartialEq, Eq)]
#[dust_dds(nested)]
pub enum PowerConnectorType {
    Unknown,
    TerminalBlock,
    Milstd,
    Nema5,
    Camlock,
    Powerlock,
    Iec60309,
    J1772,
    Powerlok,
    Milstd1651,
    Mildtl22992,
    Mildtl53126,
    Other,
    Meter,
    Bus,
}

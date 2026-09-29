use dust_dds::infrastructure::type_support::TypeSupport;

/// dust_dds patch of `tms::ConfigId` — a fieldless enum, member-only (never a topic in its own
/// right). Variant order = the IDL declaration order (0-based discriminants), matching
/// `lib_tms::generated::ConfigId` and the chains' `enum_u32` encoding. Shared by the `Reply` and
/// request-type patch chains. Derive-only: no hand impl, so this is a single-type file by design.
#[derive(TypeSupport, Clone, Copy, Debug, PartialEq, Eq)]
#[dust_dds(nested)]
pub enum ConfigId {
    Unknown,
    Defaults,
    Active,
    OnReboot,
    OnCommsLoss,
}

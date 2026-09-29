use dust_dds::infrastructure::type_support::TypeSupport;

/// dust_dds patch of `tms::ClockMonotonic` — `{ uint32 seconds; uint32 nanoseconds; }`, member-only
/// (never a topic in its own right, hence `#[dust_dds(nested)]`). Field order = the IDL member order
/// (the chains' order). Derive-only: no hand impl, so this is a single-type file by design.
#[derive(TypeSupport, Clone, Copy, Debug, PartialEq)]
#[dust_dds(nested, extensibility = "appendable")]
pub struct ClockMonotonic {
    pub seconds: u32,
    pub nanoseconds: u32,
}

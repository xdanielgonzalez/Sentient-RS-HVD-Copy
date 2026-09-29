use dust_dds::infrastructure::type_support::TypeSupport;

use super::reply_code::ReplyCode;

/// dust_dds patch of `tms::ReplyStatus` — `{ ReplyCode code; string reason; }`, member-only
/// (nested inside `Reply.status`). Field order = the IDL member order. Derive-only: no hand impl,
/// so this is a single-type file by design.
#[derive(TypeSupport, Clone, Debug, PartialEq)]
#[dust_dds(nested, extensibility = "appendable")]
pub struct ReplyStatus {
    pub code: ReplyCode,
    pub reason: String,
}

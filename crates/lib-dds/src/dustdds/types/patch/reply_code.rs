use dust_dds::infrastructure::type_support::TypeSupport;

/// dust_dds patch of `tms::ReplyCode` — a fieldless enum, member-only (nested inside
/// `ReplyStatus`). Variant order = the IDL declaration order, matching
/// `lib_tms::generated::ReplyCode` and the chains' `enum_u32` encoding. Derive-only: no hand impl,
/// so this is a single-type file by design.
#[derive(TypeSupport, Clone, Copy, Debug, PartialEq, Eq)]
#[dust_dds(nested)]
pub enum ReplyCode {
    Unknown,
    Ok,
    BadRequest,
    MethodNotAllowed,
    Conflict,
    Gone,
    PreconditionFailed,
    InternalServerError,
    NotImplemented,
    ServiceUnavailable,
    PendingAuthorization,
    NotMaster,
    PermissionDenied,
}

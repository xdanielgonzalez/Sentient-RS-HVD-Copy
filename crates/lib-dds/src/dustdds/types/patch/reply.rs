use dust_dds::infrastructure::type_support::TypeSupport;

use super::config_id::ConfigId;
use super::reply_status::ReplyStatus;

/// dust_dds patch of `tms::Reply` — `{ string requestingDeviceId @key; string targetDeviceId @key;
/// ConfigId config; long portNumber; unsigned long long requestSequenceId; ReplyStatus status; }`.
/// Every member is required (no `@optional` anywhere in this topic), so the WP4 rung-C round-trip
/// is byte-identical. The instance key is the two-field pair per `generated_qos::key_fields`. Field
/// order = the IDL member order (the chains' order). Derive-only: no hand impl, so this is a
/// single-type file by design.
#[derive(TypeSupport, Clone, Debug, PartialEq)]
#[dust_dds(extensibility = "appendable")]
pub struct Reply {
    #[dust_dds(key)]
    pub requesting_device_id: String,
    #[dust_dds(key)]
    pub target_device_id: String,
    pub config: ConfigId,
    pub port_number: i32,
    pub request_sequence_id: u64,
    pub status: ReplyStatus,
}

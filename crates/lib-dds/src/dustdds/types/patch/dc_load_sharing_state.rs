use dust_dds::infrastructure::type_support::TypeSupport;

use super::config_id::ConfigId;
use super::dc_load_sharing_parameters::DcLoadSharingParameters;

/// dust_dds patch of `tms::dc::LoadSharingState` (topic `DcLoadSharingState`) — `{ string device_id
/// @key; ConfigId config; sequence<LoadSharingParameters> parameters; }`. Every member is required
/// (no `@optional` anywhere in this topic's transitive closure — the ONLY one of the WP6.5 eight that
/// is `@optional`-free and therefore patchable via `dust_dds_derive` today), so the rung-C
/// deserialize→re-serialize round-trip is byte-identical. `parameters` is a NON-primitive-element
/// sequence carrying the rule-(12) NEXTINT delimiter on the wire (vendored dust_dds patch;
/// `vendor/dust_dds/PATCHES.md`). Field order = the IDL member order (the chains' order).
/// Derive-only: no hand impl, so this is a single-type file by design.
#[derive(TypeSupport, Clone, Debug, PartialEq)]
#[dust_dds(extensibility = "appendable")]
pub struct DcLoadSharingState {
    #[dust_dds(key)]
    pub device_id: String,
    pub config: ConfigId,
    pub parameters: Vec<DcLoadSharingParameters>,
}

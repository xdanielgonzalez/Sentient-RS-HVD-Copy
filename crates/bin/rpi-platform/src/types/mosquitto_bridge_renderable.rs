/// Warns if `--emit-mosquitto-bridge` would currently fail closed (a `[cloud]` section is
/// configured but missing a required field or the password secret). A local-only device (no
/// `[cloud]` section) passes trivially, same as [`super::CloudCredsPresent`].
pub(in super::super) struct MosquittoBridgeRenderable;

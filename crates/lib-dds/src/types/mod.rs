//! The owned DDS seam — data + trait definitions, backend-agnostic (no `dust_dds` here).
//!
//! Per the repo's Go-style split, each type/trait lives in its own file; impls at the module root
//! (`impl_*.rs`) or in the backend adapter. Re-exported flat so public paths are `lib_dds::DdsError`.

mod dds_config;
mod dds_error;
mod dds_handle;
mod dds_outbound;
mod dds_sample;
mod participant;
mod writer;

pub use dds_config::DdsConfig;
pub use dds_error::DdsError;
pub use dds_handle::DdsHandle;
pub use dds_outbound::DdsOutbound;
pub use dds_sample::DdsSample;
pub use participant::DdsParticipant;
pub use writer::DdsWriter;

/// Crate result alias. Defined here in `mod.rs` (not a `types/` leaf) deliberately: a `type` alias
/// counts as a primary type under the type/impl smell gate, so colocating it with the `DdsError`
/// enum would trip the one-type-per-file rule.
pub type Result<T> = core::result::Result<T, DdsError>;

//! Private prelude for the `dustdds` adapter's `impl_*.rs` files ONLY. Type files under `types/`
//! import directly. Re-exports the adapter's own types, the crate seam, and the dust_dds/lib_tms
//! externals the impls share. (Transport-specific externals — the `lib_net::embassy::udp` seam and
//! `dust_dds::transport` — are imported directly in the two transport impls.)

pub use super::types::*;

pub use crate::types::{DdsError, DdsParticipant, DdsSample, DdsWriter, Result};

pub use lib_tms::generated_qos::QosProfile;
pub use lib_tms::{Identity, TmsTopicId};

pub use dust_dds::infrastructure::qos::QosKind;
pub use dust_dds::infrastructure::status::NO_STATUS;
pub use dust_dds::infrastructure::type_support::TypeSupport;
pub use dust_dds::listener::NO_LISTENER;

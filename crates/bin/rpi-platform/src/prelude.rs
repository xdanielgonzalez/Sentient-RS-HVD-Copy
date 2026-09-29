//! Private glob-import surface for this crate's split files.

pub use super::types::*;

pub use lib_dds::{DdsHandle, DdsOutbound};
pub use lib_io::host::{HostSerial, SerialKind};
pub use lib_io::persist::{LogRecord, RawCaptureRecord};
pub use lib_io::transport::{Direction, IoError, SerialParams, SerialTransport};
pub use lib_mapping::render::{render_fault, render_tms, TmsTopic};
pub use lib_mapping::render_cdr::{render_fault_cdr, render_storage_update_cdr, render_tms_cdr};
// `lib_mapping::schema` is `pub use lib_tms::*` (+ 2 aliases) — `TmsTopicId`/`Identity` here are the
// SAME types `lib_dds::DdsOutbound` names, without adding a direct `lib-tms` dependency to this
// crate (it already reaches `lib-tms` transitively via `lib-mapping`/`lib-dds`).
pub use lib_mapping::schema::{topics, ClockMonotonic, Identity, TmsTopicId};
pub use lib_mapping::sink::{topic_into, MAX_PAYLOAD, MAX_TOPIC};
pub use lib_mapping::Publisher;
pub use lib_mqtt::broker::BrokerPublisher;
pub use lib_net::mesh::{condense, Condensed};
pub use lib_parse::canonical::{
    CanonicalFault, CanonicalInverterFault, CanonicalInverterReading, CanonicalReading,
    InverterMode,
};
pub use spec_app::preflight::capabilities::{ConfigCap, SerialCap};
pub use spec_app::preflight::checks::{ConfigValid, SerialPresent};
pub use spec_app::preflight::{run_checks, Check, Outcome, Preflight, Report};
pub use spec_app::{Health, Platform, RunConfig};
pub use std::time::{Duration, Instant};

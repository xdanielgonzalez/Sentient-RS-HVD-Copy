//! Background tasks publishing/answering over the DDS/RTPS transport (#324, WP6) — the DDS-side
//! counterparts of `crate::tasks::{heartbeat, state_publisher, reply_listener}`, sharing the
//! transport-neutral Request/Reply core (`crate::tasks::request_reply`) and the same per-identity
//! (D13) spawn discipline. Spawned by `crate::runtime::supervise` only when `[dds]` is configured
//! (MQTT stays always-on regardless).
//!
//! Every publish here renders the concrete TMS struct as a complete XCDR2 SerializedPayload via
//! `lib_mapping::render_cdr::render_cdr_value` and enqueues it on the shared [`lib_dds::DdsHandle`]
//! as a [`lib_dds::DdsOutbound`] — the SAME bytes-in seam `bin_rpi_platform::DdsTap` uses. A topic
//! `lib-dds` has not yet grown a writer mapping for (see `lib_dds::DdsError::UnsupportedTopic`) is
//! gracefully dropped by `lib_dds::run_dds_stack`'s pump — never a task failure; see
//! `lib_dds::dustdds::types::patch`'s module doc comment for exactly which topics are wired today
//! (`Heartbeat`, `DcMeasurementUpdate`, `Reply`) vs. deferred (`DeviceInfo`, the four Required STOR
//! state topics, and the two named request types — all `@optional`-heavy).

pub mod heartbeat;
pub mod request_listener;
pub mod state_publisher;
mod util;

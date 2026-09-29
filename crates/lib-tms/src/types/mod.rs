//! Hand-written trait + error definitions shared by the GENERATED type catalogue
//! ([`crate::generated`]). One primary type per file (CLAUDE.md "Code Organization").
//!
//! The flat-wire `WireFlat` trait + `WireError` live in `lib-mapping::wire` (alongside the generated
//! encoders), not here: `WireFlat` impls carry the canonical device model + device context (see
//! `lib_mapping::wire::WireFlat`'s doc comment), and `lib-tms` has no canonical dependency (the
//! layering-cycle rule this crate's root doc comment states). `JsonSink` itself — the abstraction
//! `WireFlat` writes through — has no such dependency and lives in [`crate::fields`] instead.

mod decode_error;
mod tms_topic;

pub use decode_error::DecodeError;
pub use tms_topic::TmsTopic;

use crate::generated::TmsTopicId;

/// A TMS type published on one or more DDS/MQTT topics. Implemented (GENERATED, in
/// [`crate::generated`]) only for the IDL structs that carry an `@topic` annotation; `@nested`
/// helper structs are plain data with no `TmsTopic` impl.
///
/// # Topic identity is an enum index, not a string
///
/// The canonical in-memory topic identity is [`crate::generated::TmsTopicId`] — a `#[repr(u8)]` enum, ONE byte. The
/// topic *string* is materialized only at an I/O boundary (`TmsTopicId::as_str` on output,
/// `TmsTopicId::from_str` on inbound parse), never stored on the hot path. This follows the repo's
/// canonical-in-memory / render-on-output doctrine (CLAUDE.md; the T3 RAM lesson): hold the compact
/// index, render the string on demand.
///
/// # Example
///
/// ```rust
/// use lib_tms::{DcMeasurementUpdate, TmsTopic, TmsTopicId};
///
/// // The identity is the 1-byte enum index; the string is derived on demand.
/// assert_eq!(DcMeasurementUpdate::TOPIC_ID, TmsTopicId::DcMeasurementUpdate);
/// assert_eq!(DcMeasurementUpdate::topic_str(), "DcMeasurementUpdate");
///
/// // A multi-topic type lists every DDS topic name it serves (generator override).
/// assert_eq!(
///     DcMeasurementUpdate::topics(),
///     ["DcMeasurementUpdate", "DcSummaryMeasurementUpdate"]
/// );
///
/// // Provenance for the traceability report: the declaring IDL line.
/// assert_eq!(DcMeasurementUpdate::IDL_LINE, 4963);
/// ```
pub trait TmsTopic {
    /// The canonical in-memory identity: a 1-byte [`crate::generated::TmsTopicId`] index (the type's PRIMARY topic,
    /// for a type that serves several — see [`TmsTopic::topics`]).
    const TOPIC_ID: TmsTopicId;

    /// The primary DDS topic-name string. A convenience mirror of `TOPIC_ID.as_str()`, kept as an
    /// associated `const` for ergonomic literal use; the *identity* is [`TmsTopic::TOPIC_ID`].
    const TOPIC: &'static str;

    /// The 1-based line in `docs/compliance/tms/mil-std-3071_data_model.idl` where this type is
    /// declared — provenance for the traceability report.
    const IDL_LINE: u32;

    /// The primary topic string, derived on demand from the enum identity (no stored string).
    fn topic_str() -> &'static str {
        Self::TOPIC_ID.as_str()
    }

    /// Every topic-name string this type is published on. Defaults to the single [`TmsTopic::TOPIC`];
    /// the generator overrides it for a multi-topic type (e.g. `dc::MeasurementUpdate` serves both
    /// `DcMeasurementUpdate` and `DcSummaryMeasurementUpdate`).
    fn topics() -> &'static [&'static str] {
        &[Self::TOPIC]
    }
}

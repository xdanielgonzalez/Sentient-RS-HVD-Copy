/// Publish error. `no_std`; the underlying sink's detail is logged by the impl, not carried here.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishError {
    /// Payload did not fit the serialization buffer.
    Serialize,
    /// Topic did not fit the topic buffer.
    TopicTooLong,
    /// The underlying sink/link failed.
    Sink,
}

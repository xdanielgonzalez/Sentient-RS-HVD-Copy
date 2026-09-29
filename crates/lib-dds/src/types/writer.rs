use super::Result;
use lib_tms::Identity;

/// A per-topic DDS writer behind the seam. Object-safe so the stack can hold `Box<dyn DdsWriter>` in
/// its lazy `(device_id, topic)` map without naming the backend's concrete (patch-type-generic)
/// writer.
///
/// [`DdsWriter::write`] takes a **pre-serialized XCDR2 SerializedPayload** (the bytes-in contract —
/// see the crate docs). The `key` is the instance key = device identity.
pub trait DdsWriter: Send {
    /// Publish `payload` (a complete XCDR2 SerializedPayload) under instance `key`.
    fn write(&self, key: &Identity, payload: &[u8]) -> Result<()>;
}

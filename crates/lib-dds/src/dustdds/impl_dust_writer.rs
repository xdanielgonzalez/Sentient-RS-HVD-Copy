use dust_dds::xtypes::deserializer::CdrDeserializer;

use super::prelude::*;

// The rung-C bytes-in write. `M: TypeSupport` is the topic patch type; `Send + 'static` so the erased
// `Box<dyn DdsWriter>` is `Send` for the stack's map (held across `.await`).
impl<M: TypeSupport + Send + 'static> DdsWriter for DustWriter<M> {
    /// Publish a pre-serialized XCDR2 payload. The instance key (deviceId) is the `@key` member
    /// carried INSIDE the payload — dust_dds derives the instance handle from the reconstituted
    /// sample — so `_key` is redundant here (kept for the seam contract).
    ///
    /// Pipeline: `payload` → `CdrDeserializer::deserialize(M::get_type(), payload)` (a `DynamicData`
    /// of the topic's type; version/endian read from the encapsulation header) → `M::create_sample`
    /// → `DataWriter::write`, which re-serializes via XCDR2 (representation pinned in the QoS). For an
    /// optional-free topic the emitted wire bytes equal `payload` byte-for-byte (WP4 round-trip proof
    /// + WP3 oracle).
    fn write(&self, _key: &Identity, payload: &[u8]) -> Result<()> {
        let dynamic = CdrDeserializer::deserialize(M::get_type(), payload)
            .map_err(|e| DdsError::Payload(format!("{e:?}")))?;
        let sample = M::create_sample(dynamic);
        self.writer
            .write(sample, None)
            .map_err(|e| DdsError::Write(format!("{e:?}")))?;
        Ok(())
    }
}

use dust_dds::publication::data_writer::DataWriter;

/// A dust_dds-backed [`crate::DdsWriter`] for one topic type `M` (a patch type deriving
/// `TypeSupport`). Erased behind `Box<dyn DdsWriter>` in the stack's writer map. Impl (the rung-C
/// bytes-in write) in `impl_dust_writer.rs`.
pub struct DustWriter<M> {
    pub(crate) writer: DataWriter<M>,
}

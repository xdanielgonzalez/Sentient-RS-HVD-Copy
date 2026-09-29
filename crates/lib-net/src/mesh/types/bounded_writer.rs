/// A `core::fmt::Write` cursor over a caller-supplied byte buffer — no `alloc`, no `String`/`Vec`.
///
/// Used by [`crate::mesh::impl_condense::write_fields`] to format condensed JSON fields directly
/// into the caller's output slice. See `crate::mesh::impl_bounded_writer` for the `Write` impl and
/// the checkpoint/rollback behavior (`len` is only advanced on a successful write) that lets the
/// greedy `condense` loop try a field, discover it doesn't fit, and roll back to try the NEXT
/// (possibly smaller) field instead of corrupting already-written output or stopping permanently.
pub(crate) struct BoundedWriter<'a> {
    pub(crate) buf: &'a mut [u8],
    pub(crate) len: usize,
}

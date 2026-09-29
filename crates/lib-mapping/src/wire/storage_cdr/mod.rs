//! Hand-written XCDR2 CDR view binding for the `StorageUpdate` topic — CDR twin of `wire::storage`.
//! Implements the generated `StorageUpdateCdrSrc` accessor trait with UNSCALED values; `render_cdr`
//! builds it and drives `lib_tms::cdr::emit_object_cdr`.

mod impl_storage_update;

pub use impl_storage_update::StorageUpdateCdrView;

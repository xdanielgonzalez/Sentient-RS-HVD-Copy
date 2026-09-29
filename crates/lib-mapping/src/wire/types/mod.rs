mod counting_sink;
mod wire_error;
mod wire_flat;
mod write_sink;

pub use counting_sink::CountingSink;
// `JsonSink` itself lives in `lib-tms` (a leaf abstraction with no canonical dependency, shared by
// every wire encoder — see `lib_tms::fields`'s module doc comment); re-exported here so the public
// `lib_mapping::wire::JsonSink` path is unchanged.
pub use lib_tms::fields::JsonSink;
pub use wire_error::WireError;
pub use wire_flat::WireFlat;
pub use write_sink::WriteSink;

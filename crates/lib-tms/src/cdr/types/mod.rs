//! XCDR2 core type definitions (CLAUDE.md "Code Organization": one primary type per file, no
//! `type_` prefix — the `types/` module already marks these as definitions). Impls live in
//! `impl_*.rs` at the `cdr/` module root; free helper functions live in `cdr/mod.rs`.

mod cdr_chain;
mod cdr_counting_sink;
mod cdr_error;
mod cdr_field;
mod cdr_nil;
mod cdr_node;
mod cdr_reader;
mod cdr_sink;
mod cdr_write_sink;
mod has_cdr_chain;

pub use cdr_chain::CdrChain;
pub use cdr_counting_sink::CdrCountingSink;
pub use cdr_error::CdrError;
pub use cdr_field::CdrField;
pub use cdr_nil::CdrNil;
pub use cdr_node::CdrNode;
pub use cdr_reader::CdrReader;
pub use cdr_sink::CdrSink;
pub use cdr_write_sink::CdrWriteSink;
pub use has_cdr_chain::HasCdrChain;

//! `impl FieldChain for Nil` — the type-level list terminator, see `types/nil.rs`'s doc comment.

use super::prelude::*;

impl<Src: ?Sized> FieldChain<Src> for Nil {
    const LEN: usize = 0;

    #[inline(always)]
    fn emit_fields<S: JsonSink>(_src: &Src, _sink: &mut S, _first: &mut bool) {}
}

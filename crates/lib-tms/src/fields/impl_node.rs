//! `impl FieldChain for Node` — the type-level list cons cell, see `types/node.rs`'s doc comment.

use super::prelude::*;

impl<Src: ?Sized, F: Field<Src>, Rest: FieldChain<Src>> FieldChain<Src> for Node<F, Rest> {
    const LEN: usize = 1 + Rest::LEN;

    #[inline(always)]
    fn emit_fields<S: JsonSink>(src: &Src, sink: &mut S, first: &mut bool) {
        F::emit(src, sink, first);
        Rest::emit_fields(src, sink, first);
    }
}

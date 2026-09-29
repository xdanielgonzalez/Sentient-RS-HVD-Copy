//! `impl CdrChain for CdrNode` — the type-level list cons cell. See `types/cdr_node.rs`.

use super::prelude::*;

impl<Src: ?Sized, F: CdrField<Src>, Rest: CdrChain<Src>> CdrChain<Src> for CdrNode<F, Rest> {
    const LEN: usize = 1 + Rest::LEN;

    #[inline(always)]
    fn emit<S: CdrSink>(src: &Src, sink: &mut S) {
        F::emit(src, sink);
        Rest::emit(src, sink);
    }
}

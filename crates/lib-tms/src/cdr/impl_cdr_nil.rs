//! `impl CdrChain for CdrNil` — the type-level list terminator. See `types/cdr_nil.rs`.

use super::prelude::*;

impl<Src: ?Sized> CdrChain<Src> for CdrNil {
    const LEN: usize = 0;

    #[inline(always)]
    fn emit<S: CdrSink>(_src: &Src, _sink: &mut S) {}
}

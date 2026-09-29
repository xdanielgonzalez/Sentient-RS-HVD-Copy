//! [`CdrNode`] — a type-level cons cell of the XCDR2 field list. See `crate::cdr::impl_cdr_node`
//! for its [`super::CdrChain`] impl.

use core::marker::PhantomData;

/// A type-level cons cell: field `F` prepended onto the rest of the chain `Rest`. Never
/// instantiated — `F`/`Rest` are pure type parameters walked at compile time (see `crate::cdr`'s
/// module doc), so the `PhantomData` payload is zero-sized and carries no runtime data.
pub struct CdrNode<F, Rest>(PhantomData<(F, Rest)>);

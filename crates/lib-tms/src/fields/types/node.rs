use core::marker::PhantomData;

/// A type-level cons cell: field `F` prepended onto the rest of the chain `Rest`. Never
/// instantiated — `F`/`Rest` are pure type parameters walked at compile time (see `crate::fields`'s
/// module doc comment), so the `PhantomData` payload is zero-sized and carries no runtime data. See
/// `crate::fields::impl_node` for its [`super::field_chain::FieldChain`] impl.
pub struct Node<F, Rest>(PhantomData<(F, Rest)>);

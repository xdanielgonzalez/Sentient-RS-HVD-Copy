use super::field_chain::FieldChain;

/// Associates a source type with its compile-time [`FieldChain`] — the entry point generated code
/// uses to name "the field list for this struct" without spelling out the full `Node<F1, Node<F2,
/// …>>` chain type at every call site. Implementations are emitted by the IDL codegen, one per
/// generated struct.
pub trait HasChain: Sized {
    /// The field chain naming every field of `Self`, in IDL declaration order.
    type Chain: FieldChain<Self>;
}

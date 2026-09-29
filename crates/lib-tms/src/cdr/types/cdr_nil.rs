//! [`CdrNil`] — the empty [`super::CdrChain`], i.e. the type-level list terminator. See
//! `crate::cdr::impl_cdr_nil` for its [`super::CdrChain`] impl.

/// The empty [`super::CdrChain`] — the terminator of a type-level field list. Never instantiated.
pub struct CdrNil;

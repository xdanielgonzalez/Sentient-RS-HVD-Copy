use super::tls_error::TlsError;

/// The crate-wide result alias. Every fallible operation in `lib-tls` returns this, and while the
/// crate is a skeleton every one of them returns [`TlsError::Unimplemented`].
pub type Result<T> = core::result::Result<T, TlsError>;

/// An opaque reference to a key held by a [`super::key_store::KeyStore`]. Carries no key material — it is a handle
/// into the store's custody domain (secure world or software memory). `Copy` so it is cheap to pass
/// around, but it is meaningless outside its originating store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyHandle(pub u32);
